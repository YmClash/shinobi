//! Producteur Kafka Chakra — Publication d'événements webhook.
//!
//! Publie les événements webhook sur le topic `shinobi.events.webhooks`.
//! Réutilise le pattern `KafkaEventPublisher` existant.
//!
//! ## Topic
//! - `shinobi.events.webhooks` : événements webhook (push, MR, issues)
//!
//! ## Clé de partitionnement
//! `repository_id` — garantit que tous les événements d'un repo arrivent
//! sur la même partition Kafka (ordre garanti par repo).

use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use tracing::{info, warn};

use domain::entities::webhook::WebhookEvent;
use domain::errors::DomainError;

/// Producteur Kafka pour les événements webhook Chakra.
///
/// Thread-safe (`Send + Sync`) — le `FutureProducer` est conçu
/// pour être partagé via `Arc`.
pub struct ChakraProducer {
    producer: FutureProducer,
    topic: String,
}

impl ChakraProducer {
    /// Crée un nouveau producteur Chakra.
    ///
    /// # Arguments
    /// - `brokers` : liste de brokers Kafka (ex: `"localhost:9092"`)
    /// - `topic` : topic webhook (ex: `"shinobi.events.webhooks"`)
    pub fn new(brokers: &str, topic: &str) -> Result<Self, DomainError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.ms", "100")
            .create()
            .map_err(|e| {
                DomainError::Internal(format!("Chakra Kafka producer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            "🔔 ChakraProducer initialisé (connexion lazy)"
        );

        Ok(Self {
            producer,
            topic: topic.to_string(),
        })
    }

    /// Publie un événement webhook sur le topic Kafka.
    ///
    /// Fire-and-forget : ne bloque jamais le thread principal.
    /// Si Kafka est down, l'événement est perdu (at-most-once).
    /// Le développeur a déjà sa réponse HTTP 200.
    pub async fn publish(&self, event: &WebhookEvent) -> Result<(), DomainError> {
        let payload = serde_json::to_string(event).map_err(|e| {
            DomainError::Internal(format!("Chakra JSON serialization failed: {e}"))
        })?;

        // Clé = repository_id → même partition pour un repo donné
        let key = event.repository_id.to_string();

        let delivery_result = self
            .producer
            .send(
                FutureRecord::to(&self.topic)
                    .key(&key)
                    .payload(&payload),
                Duration::from_secs(5),
            )
            .await;

        match delivery_result {
            Ok(delivery) => {
                info!(
                    topic = %self.topic,
                    partition = delivery.partition,
                    offset = delivery.offset,
                    event_id = %event.id,
                    event_type = %event.event_type,
                    repository_id = %event.repository_id,
                    "🔔 Chakra — Événement webhook publié sur Kafka"
                );
                Ok(())
            }
            Err((kafka_error, _)) => {
                warn!(
                    topic = %self.topic,
                    event_id = %event.id,
                    error = %kafka_error,
                    "⚠️ Chakra — Échec publication Kafka — événement webhook perdu"
                );
                Err(DomainError::Internal(format!(
                    "Chakra Kafka publish failed: {kafka_error}"
                )))
            }
        }
    }
}
