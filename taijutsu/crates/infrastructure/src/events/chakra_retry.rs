//! Chakra Retry Worker — Régénération des shurikens ratés 🔄
//!
//! Consomme le sorted set Redis `shinobi:chakra:retries` et
//! re-dispatch les webhooks en échec avec backoff exponentiel.
//!
//! ## Fonctionnement
//! - Poll le ZSET toutes les 10 secondes
//! - Prend les entrées dont le score (timestamp) est ≤ maintenant
//! - Re-dispatch via `ChakraDispatcher::dispatch_to_webhook()`
//! - Si 5ème échec → DLQ (le dispatcher le gère)
//!
//! ## Redis ZSET
//! ```text
//! ZADD shinobi:chakra:retries <timestamp_next_attempt> <json_payload>
//! ZRANGEBYSCORE shinobi:chakra:retries -inf <now> LIMIT 0 10
//! ZREM shinobi:chakra:retries <member>
//! ```

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use domain::entities::webhook::{Webhook, WebhookRetryPayload};

use super::chakra_dispatcher::{ChakraDispatcher, CHAKRA_RETRY_ZSET_KEY};
use crate::cache::redis_cache::RedisCache;
use domain::ports::webhook_repository::WebhookRepository;

/// Intervalle de polling du ZSET Redis (en secondes).
const POLL_INTERVAL_SECS: u64 = 10;

/// Nombre maximum de retries traités par cycle de poll.
const MAX_RETRIES_PER_POLL: usize = 10;

/// Worker de retry pour le système Chakra.
///
/// Tourne en boucle et consomme les retries planifiés dans Redis.
/// Utilise `CancellationToken` pour le shutdown gracieux.
pub struct ChakraRetryWorker {
    redis: RedisCache,
    dispatcher: Arc<ChakraDispatcher>,
    webhook_repo: Arc<dyn WebhookRepository>,
    cancel_token: CancellationToken,
}

impl ChakraRetryWorker {
    /// Construit un nouveau retry worker.
    pub fn new(
        redis: RedisCache,
        dispatcher: Arc<ChakraDispatcher>,
        webhook_repo: Arc<dyn WebhookRepository>,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            redis,
            dispatcher,
            webhook_repo,
            cancel_token,
        }
    }

    /// Démarre la boucle de retry.
    pub async fn start(&self) {
        info!("🔄 Chakra Retry Worker — Démarré (poll toutes les {POLL_INTERVAL_SECS}s)");

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("🛑 Chakra Retry Worker — Shutdown gracieux");
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)) => {
                    self.process_due_retries().await;
                }
            }
        }

        info!("🔄 Chakra Retry Worker — Arrêté proprement");
    }

    /// Traite les retries dont le timestamp est dépassé.
    async fn process_due_retries(&self) {
        let now = chrono::Utc::now().timestamp();
        let mut conn = self.redis.clone_connection();

        // Récupérer les retries dus
        let entries: Vec<String> = match redis::cmd("ZRANGEBYSCORE")
            .arg(CHAKRA_RETRY_ZSET_KEY)
            .arg("-inf")
            .arg(now)
            .arg("LIMIT")
            .arg(0)
            .arg(MAX_RETRIES_PER_POLL)
            .query_async(&mut conn)
            .await
        {
            Ok(entries) => entries,
            Err(e) => {
                // Redis down — pas grave, on réessaie au prochain cycle
                if !format!("{e}").contains("connection") {
                    warn!(error = %e, "⚠️ Chakra Retry — Erreur ZRANGEBYSCORE");
                }
                return;
            }
        };

        if entries.is_empty() {
            return; // Rien à traiter
        }

        info!(
            count = entries.len(),
            "🔄 Chakra Retry — {} webhook(s) à relancer",
            entries.len()
        );

        for entry in &entries {
            // Désérialiser le payload de retry
            let retry: WebhookRetryPayload = match serde_json::from_str(entry) {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "⚠️ Chakra Retry — Payload corrompu, skip");
                    // Supprimer l'entrée corrompue
                    let _: Result<(), _> = redis::cmd("ZREM")
                        .arg(CHAKRA_RETRY_ZSET_KEY)
                        .arg(entry)
                        .query_async(&mut conn)
                        .await;
                    continue;
                }
            };

            // Supprimer l'entrée du ZSET AVANT de la traiter
            // (pour éviter le double-traitement si le worker crashe)
            let _: Result<(), redis::RedisError> = redis::cmd("ZREM")
                .arg(CHAKRA_RETRY_ZSET_KEY)
                .arg(entry)
                .query_async(&mut conn)
                .await;

            // Reconstruire le webhook minimal pour le dispatch
            let webhook = Webhook {
                id: retry.webhook_id,
                repository_id: retry.event.repository_id,
                creator_id: uuid::Uuid::nil(), // Non utilisé pour le dispatch
                url: retry.url,
                secret: retry.secret,
                events: vec![retry.event.event_type],
                active: true,
                last_delivery_at: None,
                failure_count: 0,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            info!(
                webhook_id = %retry.webhook_id,
                event_id = %retry.event.id,
                attempt = retry.next_attempt,
                "🔄 Chakra Retry — Tentative #{}",
                retry.next_attempt
            );

            // Re-dispatch
            self.dispatcher
                .dispatch_to_webhook(&webhook, &retry.event, retry.next_attempt)
                .await;
        }
    }
}
