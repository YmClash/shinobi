//! Chakra Dispatcher — Le Lanceur de Shuriken 🌀
//!
//! Dispatcher HTTP sécurisé pour les webhooks. Responsabilités :
//! 1. **SSRF Protection** : Bloque les requêtes vers les réseaux privés
//! 2. **HMAC-SHA256 Signing** : Signe chaque payload avec le secret du webhook
//! 3. **CloudEvents Headers** : Compatible CNCF CloudEvents 1.0
//! 4. **Audit Trail** : Enregistre chaque livraison (succès/échec) en DB
//! 5. **Retry Scheduling** : Planifie les retries en Redis (backoff exponentiel)
//!
//! ## Sécurité
//! - Timeout impitoyable : 10s max par requête (connect + total)
//! - Pas de redirect (évite les rebonds SSRF)
//! - Vérification réseau avant chaque POST
//!
//! ## Headers envoyés
//! | Header | Valeur |
//! |---|---|
//! | `Content-Type` | `application/json` |
//! | `X-Shinobi-Event` | `push`, `mr_created`, etc. |
//! | `X-Shinobi-Delivery` | UUID de la livraison |
//! | `X-Shinobi-Signature` | `sha256=<HMAC hex>` |
//! | `User-Agent` | `Shinobi-Webhooks/1.0` |
//! | `ce-specversion` | `1.0` |
//! | `ce-type` | `dev.jjshinobi.push` (CloudEvents) |
//! | `ce-source` | `https://api.jjshinobi.dev` |
//! | `ce-id` | UUID de l'événement |
//! | `ce-time` | ISO 8601 timestamp |

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{info, warn, error};

use domain::entities::webhook::{
    Webhook, WebhookDelivery, WebhookEvent, WebhookRetryPayload,
};
use domain::ports::webhook_repository::WebhookRepository;
use crate::cache::redis_cache::RedisCache;

/// Type HMAC-SHA256.
type HmacSha256 = Hmac<Sha256>;

/// Dispatcher HTTP sécurisé pour les webhooks Chakra.
///
/// Chaque instance est thread-safe et partageable via `Arc`.
pub struct ChakraDispatcher {
    /// Client HTTP réutilisable (pool de connexions).
    client: reqwest::Client,
    /// Cache Redis pour le retry backoff.
    redis: RedisCache,
    /// Repository PostgreSQL pour l'audit trail.
    webhook_repo: Arc<dyn WebhookRepository>,
    /// Domaine de la forge (pour les headers CloudEvents).
    federation_domain: String,
    /// Autoriser les webhooks vers localhost (dev only).
    allow_local: bool,
}

/// Clé Redis pour le sorted set de retries.
const RETRY_ZSET_KEY: &str = "shinobi:chakra:retries";

impl ChakraDispatcher {
    /// Construit un nouveau dispatcher Chakra.
    ///
    /// # Arguments
    /// - `redis` : connexion Redis pour les retries
    /// - `webhook_repo` : repository pour l'audit trail
    /// - `federation_domain` : domaine de la forge (ex: `api.jjshinobi.dev`)
    /// - `allow_local` : autoriser les webhooks vers localhost (dev only)
    pub fn new(
        redis: RedisCache,
        webhook_repo: Arc<dyn WebhookRepository>,
        federation_domain: String,
        allow_local: bool,
    ) -> Self {
        let client = reqwest::Client::builder()
            // Timeout impitoyable : 10s max (connect + read + write)
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            // Pas de redirect — empêche les rebonds SSRF
            .redirect(reqwest::redirect::Policy::none())
            // User-Agent identifiant
            .user_agent("Shinobi-Webhooks/1.0")
            .build()
            .expect("Failed to build HTTP client for Chakra dispatcher");

        Self {
            client,
            redis,
            webhook_repo,
            federation_domain,
            allow_local,
        }
    }

    /// Dispatche un événement webhook vers tous les endpoints abonnés.
    ///
    /// Pour chaque webhook actif abonné au type d'événement :
    /// 1. Vérifie que l'URL est sûre (anti-SSRF)
    /// 2. Signe le payload HMAC-SHA256
    /// 3. Envoie le POST avec les headers CloudEvents
    /// 4. Enregistre le résultat en DB
    /// 5. Planifie un retry en Redis si échec
    pub async fn dispatch_event(&self, event: &WebhookEvent) {
        // Lookup des webhooks actifs pour cet événement
        let webhooks = match self
            .webhook_repo
            .find_active_for_event(
                &event.repository_id,
                event.event_type.as_sql_str(),
            )
            .await
        {
            Ok(wh) => wh,
            Err(e) => {
                error!(
                    event_id = %event.id,
                    error = %e,
                    "❌ Chakra — Échec lookup webhooks"
                );
                return;
            }
        };

        if webhooks.is_empty() {
            return; // Pas de webhook abonné → rien à faire
        }

        info!(
            event_id = %event.id,
            event_type = %event.event_type,
            webhook_count = webhooks.len(),
            "🔔 Chakra — Dispatching vers {} webhook(s)",
            webhooks.len()
        );

        for webhook in &webhooks {
            self.dispatch_to_webhook(webhook, event, 1).await;
        }
    }

    /// Dispatche un événement vers un webhook spécifique.
    ///
    /// Appelé aussi par le retry worker pour les tentatives suivantes.
    pub async fn dispatch_to_webhook(
        &self,
        webhook: &Webhook,
        event: &WebhookEvent,
        attempt: i16,
    ) {
        // ── 1. Vérification SSRF ──────────────────────────────────
        if !self.is_safe_url(&webhook.url) {
            warn!(
                webhook_id = %webhook.id,
                url = %webhook.url,
                "⛔ Chakra — URL bloquée (SSRF protection)"
            );
            return;
        }

        // ── 2. Sérialiser le payload ──────────────────────────────
        let payload_json = serde_json::to_string(&event.payload).unwrap_or_default();
        let payload_bytes = payload_json.as_bytes();

        // ── 3. Signer HMAC-SHA256 ─────────────────────────────────
        let signature = sign_payload(&webhook.secret, payload_bytes);

        // ── 4. Construire la livraison (audit trail) ──────────────
        let delivery_id = uuid::Uuid::new_v4();
        let request_headers = serde_json::json!({
            "Content-Type": "application/json",
            "X-Shinobi-Event": event.event_type.as_sql_str(),
            "X-Shinobi-Delivery": delivery_id.to_string(),
            "X-Shinobi-Signature": signature,
            "User-Agent": "Shinobi-Webhooks/1.0",
            "ce-specversion": "1.0",
            "ce-type": event.event_type.as_cloudevent_type(),
            "ce-source": format!("https://{}", self.federation_domain),
            "ce-id": event.id.to_string(),
            "ce-time": event.created_at.to_rfc3339(),
        });

        let mut delivery = WebhookDelivery::new(
            webhook.id,
            event.event_type,
            event.id,
            webhook.url.clone(),
            request_headers,
            payload_json.clone(),
            attempt,
        );
        delivery.id = delivery_id;

        // ── 5. Envoyer le POST ────────────────────────────────────
        let start = Instant::now();
        let result = self
            .client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-Shinobi-Event", event.event_type.as_sql_str())
            .header("X-Shinobi-Delivery", delivery_id.to_string())
            .header("X-Shinobi-Signature", &signature)
            // CloudEvents headers (CNCF spec)
            .header("ce-specversion", "1.0")
            .header("ce-type", event.event_type.as_cloudevent_type())
            .header("ce-source", format!("https://{}", self.federation_domain))
            .header("ce-id", event.id.to_string())
            .header("ce-time", event.created_at.to_rfc3339())
            .body(payload_json)
            .send()
            .await;

        let elapsed_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(response) => {
                let status = response.status().as_u16() as i16;
                let body = response
                    .text()
                    .await
                    .unwrap_or_default()
                    .chars()
                    .take(10240) // Tronquer à 10KB
                    .collect::<String>();

                if (200..300).contains(&(status as u16)) {
                    // ✅ Succès
                    delivery.mark_success(status, Some(body), None, elapsed_ms);
                    info!(
                        webhook_id = %webhook.id,
                        delivery_id = %delivery_id,
                        status = status,
                        duration_ms = elapsed_ms,
                        attempt = attempt,
                        "✅ Chakra — Livraison réussie"
                    );

                    // Reset failure counter + update last_delivery
                    let _ = self.webhook_repo.update_failure_count(&webhook.id, true).await;
                    let _ = self.webhook_repo.update_last_delivery(&webhook.id).await;
                } else {
                    // ⚠️ Réponse non-2xx
                    let error_msg = format!("HTTP {status}");
                    delivery.mark_failure(Some(status), Some(body), Some(elapsed_ms), error_msg.clone());
                    warn!(
                        webhook_id = %webhook.id,
                        delivery_id = %delivery_id,
                        status = status,
                        attempt = attempt,
                        "⚠️ Chakra — Livraison échouée (HTTP {status})"
                    );

                    self.handle_failure(webhook, event, attempt).await;
                }
            }
            Err(e) => {
                // ❌ Erreur réseau (timeout, DNS, etc.)
                let error_msg = format!("{e}");
                delivery.mark_failure(None, None, Some(elapsed_ms), error_msg.clone());
                warn!(
                    webhook_id = %webhook.id,
                    delivery_id = %delivery_id,
                    error = %e,
                    attempt = attempt,
                    "❌ Chakra — Erreur réseau"
                );

                self.handle_failure(webhook, event, attempt).await;
            }
        }

        // ── 6. Persister l'audit trail ────────────────────────────
        if let Err(e) = self.webhook_repo.save_delivery(&delivery).await {
            error!(
                delivery_id = %delivery_id,
                error = %e,
                "❌ Chakra — Échec persistence delivery"
            );
        }
    }

    /// Gère un échec de livraison : incrémente le failure_count
    /// et planifie un retry ou envoie en DLQ.
    async fn handle_failure(
        &self,
        webhook: &Webhook,
        event: &WebhookEvent,
        attempt: i16,
    ) {
        // Incrémenter le compteur d'échecs
        let _ = self.webhook_repo.update_failure_count(&webhook.id, false).await;

        let max_retries: i16 = 5;
        if attempt < max_retries {
            // Planifier un retry via Redis ZSET
            let next_attempt = attempt + 1;
            let backoff = WebhookRetryPayload::backoff_seconds(next_attempt);
            let retry_at = chrono::Utc::now().timestamp() + backoff as i64;

            let retry_payload = WebhookRetryPayload {
                webhook_id: webhook.id,
                event: event.clone(),
                url: webhook.url.clone(),
                secret: webhook.secret.clone(),
                next_attempt,
            };

            if let Ok(json) = serde_json::to_string(&retry_payload) {
                let mut conn = self.redis.clone_connection();
                let result: Result<(), redis::RedisError> = redis::cmd("ZADD")
                    .arg(RETRY_ZSET_KEY)
                    .arg(retry_at)
                    .arg(&json)
                    .query_async(&mut conn)
                    .await;

                match result {
                    Ok(()) => {
                        info!(
                            webhook_id = %webhook.id,
                            next_attempt = next_attempt,
                            retry_in_secs = backoff,
                            "🔄 Chakra — Retry planifié dans {}s",
                            backoff
                        );
                    }
                    Err(e) => {
                        error!(
                            webhook_id = %webhook.id,
                            error = %e,
                            "❌ Chakra — Échec planification retry Redis"
                        );
                    }
                }
            }
        } else {
            // 5ème échec → DLQ (log pour l'instant, topic DLQ sera ajouté plus tard)
            error!(
                webhook_id = %webhook.id,
                event_id = %event.id,
                "☠️ Chakra — DLQ : webhook épuisé après {} tentatives",
                attempt
            );
            // TODO: Publier sur topic DLQ `shinobi.events.webhooks.dlq`
        }
    }

    /// Vérifie qu'une URL est sûre (anti-SSRF).
    ///
    /// Bloque les requêtes vers :
    /// - Réseaux privés : 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    /// - Loopback IPv6 : ::1
    /// - Domaines internes : .local, .internal, .localhost
    /// - Schéma non-HTTPS (sauf localhost en mode dev)
    fn is_safe_url(&self, url: &str) -> bool {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => return false,
        };

        // Schéma HTTPS obligatoire (sauf localhost en mode dev)
        let scheme = parsed.scheme();
        let host = match parsed.host_str() {
            Some(h) => h,
            None => return false,
        };

        if scheme != "https" {
            if self.allow_local && scheme == "http" && is_localhost(host) {
                // Autorisé en mode dev
            } else {
                return false;
            }
        }

        // Domaines internes bloqués
        if host.ends_with(".local")
            || host.ends_with(".internal")
            || host.ends_with(".localhost")
        {
            return false;
        }

        // Vérification IP
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_ip(&ip) {
                // Exception : localhost autorisé en mode dev
                if self.allow_local && ip.is_loopback() {
                    return true;
                }
                return false;
            }
        }

        // Vérification hostname = "localhost"
        if is_localhost(host) {
            return self.allow_local;
        }

        true
    }
}

// ── HMAC-SHA256 Signing ───────────────────────────────────────────────

/// Signe un payload avec HMAC-SHA256.
///
/// Retourne le header `X-Shinobi-Signature` au format `sha256=<hex>`.
/// Le client vérifie en calculant le même HMAC avec son secret partagé.
pub fn sign_payload(secret: &str, payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(payload);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

// ── IP Safety Helpers ─────────────────────────────────────────────────

/// Vérifie si une IP est dans un réseau privé (RFC 1918 + loopback).
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()          // 127.0.0.0/8
                || v4.is_private()    // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local() // 169.254.0.0/16
                || v4.is_unspecified() // 0.0.0.0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()          // ::1
                || v6.is_unspecified() // ::
        }
    }
}

/// Vérifie si un hostname est "localhost".
fn is_localhost(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1"
}

// ── Constantes publiques ──────────────────────────────────────────────

/// Clé Redis pour le sorted set de retries (exportée pour le retry worker).
pub const CHAKRA_RETRY_ZSET_KEY: &str = RETRY_ZSET_KEY;

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_payload() {
        let signature = sign_payload("my-secret", b"hello world");
        assert!(signature.starts_with("sha256="));
        assert_eq!(signature.len(), 7 + 64); // "sha256=" + 64 hex chars

        // Deterministic
        let signature2 = sign_payload("my-secret", b"hello world");
        assert_eq!(signature, signature2);

        // Different secret → different signature
        let signature3 = sign_payload("other-secret", b"hello world");
        assert_ne!(signature, signature3);

        // Different payload → different signature
        let signature4 = sign_payload("my-secret", b"hello world!");
        assert_ne!(signature, signature4);
    }

    #[test]
    fn test_is_private_ip_v4() {
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_ip(&"172.31.255.255".parse().unwrap()));
        assert!(is_private_ip(&"192.168.1.1".parse().unwrap()));
        assert!(is_private_ip(&"169.254.1.1".parse().unwrap()));
        assert!(is_private_ip(&"0.0.0.0".parse().unwrap()));

        // Public IPs
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip(&"1.1.1.1".parse().unwrap()));
        assert!(!is_private_ip(&"172.32.0.1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ip_v6() {
        assert!(is_private_ip(&"::1".parse().unwrap()));
        assert!(is_private_ip(&"::".parse().unwrap()));

        // Public IPv6
        assert!(!is_private_ip(&"2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn test_is_localhost() {
        assert!(is_localhost("localhost"));
        assert!(is_localhost("127.0.0.1"));
        assert!(is_localhost("::1"));
        assert!(!is_localhost("example.com"));
        assert!(!is_localhost("192.168.1.1"));
    }

    // Note: is_safe_url tests require a ChakraDispatcher instance,
    // which needs Redis — these are tested in integration tests.
}
