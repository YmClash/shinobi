//! Chakra Consumer — Consommation des événements webhook via Kafka.
//!
//! Écoute le topic `shinobi.events.webhooks` et dispatche les événements
//! vers les endpoints abonnés via le `ChakraDispatcher`.
//!
//! ## Architecture Découplée (mpsc)
//!
//! ```text
//! ┌─────────────────────┐     ┌──────────────────────────┐
//! │  Consumer (rapide)   │     │  Worker Pool (N=4)        │
//! │  Kafka recv()        │     │  rx.recv()                │
//! │  → tx.send(event)   ├────►│  → dispatcher.dispatch()  │
//! │  → offset validé ✅  │ mpsc│  → sign + POST + audit    │
//! │  → retour écouter    │     │  → retry on failure       │
//! └─────────────────────┘     └──────────────────────────┘
//! ```
//!
//! ## Différence avec Tensai/Oracle
//! - **Pool de workers** (4 par défaut) au lieu d'un seul
//!   → les webhooks sont I/O bound (HTTP POST), pas CPU bound
//! - Consumer group : `shinobi-chakra-dispatcher`

use std::sync::Arc;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use metrics::counter;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use domain::entities::webhook::WebhookEvent;
use domain::errors::DomainError;

use super::chakra_dispatcher::ChakraDispatcher;

/// Taille du buffer mpsc — salle d'attente interne.
/// 64 messages pour absorber les pics (un push peut générer
/// des events pour N webhooks).
const CHANNEL_BUFFER_SIZE: usize = 64;

/// Consumer Kafka pour le système Chakra.
///
/// Consomme les événements webhook du topic `shinobi.events.webhooks`
/// et les dispatche via un pool de workers.
pub struct ChakraConsumer {
    consumer: StreamConsumer,
    topic: String,
    cancel_token: CancellationToken,
    worker_count: usize,
}

impl ChakraConsumer {
    /// Crée un nouveau consumer Chakra.
    pub fn new(
        brokers: &str,
        topic: &str,
        group_id: &str,
        cancel_token: CancellationToken,
        worker_count: usize,
    ) -> Result<Self, DomainError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "true")
            .set("auto.commit.interval.ms", "5000")
            .set("session.timeout.ms", "30000")
            .set("max.poll.interval.ms", "900000")
            .create()
            .map_err(|e| {
                DomainError::Internal(format!("Chakra Kafka consumer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group_id = %group_id,
            worker_count = worker_count,
            "🔔 ChakraConsumer initialisé (pool de {} workers)",
            worker_count
        );

        Ok(Self {
            consumer,
            topic: topic.to_string(),
            cancel_token,
            worker_count,
        })
    }

    /// Démarre la boucle de consommation Chakra avec pool de workers.
    pub async fn start(&self, dispatcher: Arc<ChakraDispatcher>) -> Result<(), DomainError> {
        self.consumer
            .subscribe(&[&self.topic])
            .map_err(|e| {
                DomainError::Internal(format!(
                    "Chakra Kafka subscribe failed on topic '{}': {e}",
                    self.topic
                ))
            })?;

        // ── Canal mpsc borné ─────────────────────────────────────
        let (tx, rx) = mpsc::channel::<WebhookEvent>(CHANNEL_BUFFER_SIZE);
        let rx = Arc::new(tokio::sync::Mutex::new(rx));

        // ── Pool de workers ──────────────────────────────────────
        let mut worker_handles = Vec::new();
        for worker_id in 0..self.worker_count {
            let worker_cancel = self.cancel_token.clone();
            let worker_rx = Arc::clone(&rx);
            let worker_dispatcher = Arc::clone(&dispatcher);
            let topic_clone = self.topic.clone();

            let handle = tokio::spawn(async move {
                info!("🔔 Chakra Worker #{worker_id} — Démarré");

                loop {
                    let event = {
                        let mut rx_guard = worker_rx.lock().await;
                        tokio::select! {
                            _ = worker_cancel.cancelled() => {
                                info!("🛑 Chakra Worker #{worker_id} — Shutdown gracieux");
                                break;
                            }
                            maybe_event = rx_guard.recv() => {
                                match maybe_event {
                                    Some(e) => e,
                                    None => {
                                        info!("🔔 Chakra Worker #{worker_id} — Canal fermé, arrêt");
                                        break;
                                    }
                                }
                            }
                        }
                    };

                    info!(
                        worker_id = worker_id,
                        event_id = %event.id,
                        event_type = %event.event_type,
                        "🔔 Chakra Worker #{worker_id} — Traitement démarré"
                    );

                    worker_dispatcher.dispatch_event(&event).await;

                    counter!(
                        "chakra_events_dispatched_total",
                        "topic" => topic_clone.clone(),
                        "worker" => worker_id.to_string()
                    ).increment(1);
                }

                info!("🔔 Chakra Worker #{worker_id} — Arrêté proprement");
            });

            worker_handles.push(handle);
        }

        info!(
            topic = %self.topic,
            buffer_size = CHANNEL_BUFFER_SIZE,
            worker_count = self.worker_count,
            "🔔 Chakra Consumer — Boucle de consommation démarrée"
        );

        // ── Consumer : polling Kafka rapide → dispatch dans le canal ──
        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("🛑 Chakra Consumer — Shutdown gracieux déclenché");
                    break;
                }
                message_result = self.consumer.recv() => {
                    match message_result {
                        Ok(message) => {
                            let payload = match message.payload_view::<str>() {
                                Some(Ok(text)) => text,
                                Some(Err(e)) => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        error = %e,
                                        "⚠️ Chakra — Payload non-UTF8, skip"
                                    );
                                    continue;
                                }
                                None => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        "⚠️ Chakra — Message sans payload, skip"
                                    );
                                    continue;
                                }
                            };

                            match serde_json::from_str::<WebhookEvent>(payload) {
                                Ok(event) => {
                                    info!(
                                        event_id = %event.id,
                                        event_type = %event.event_type,
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        "📨 Chakra — Événement webhook reçu → file d'attente"
                                    );

                                    counter!(
                                        "kafka_messages_consumed_total",
                                        "consumer" => "chakra",
                                        "topic" => self.topic.clone()
                                    ).increment(1);

                                    if let Err(e) = tx.send(event).await {
                                        error!(
                                            event_id = %e.0.id,
                                            "❌ Chakra — Canal mpsc fermé, événement perdu"
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        error = %e,
                                        "⚠️ Chakra — Désérialisation WebhookEvent échouée"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                "❌ Chakra — Erreur Kafka recv()"
                            );
                            counter!("kafka_consumer_errors_total", "consumer" => "chakra")
                                .increment(1);
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }

        // ── Cleanup ──────────────────────────────────────────────
        drop(tx);
        for handle in worker_handles {
            let _ = handle.await;
        }

        info!("🔔 Chakra Consumer — Boucle arrêtée proprement");
        Ok(())
    }
}
