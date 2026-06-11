//! Adaptateur Nen — Publication événementielle via Apache Kafka.
//!
//! Utilise `rdkafka` (wrapper librdkafka) pour publier des événements
//! JSON sur le topic `shinobi.vcs.operations`.
//!
//! ## Architecture
//! - `FutureProducer` : producteur async natif rdkafka (compatible tokio).
//! - Clé de partitionnement : `operation.id` (UUID) — garantit que
//!   toutes les mutations d'une opération arrivent sur la même partition.
//! - Payload : JSON sérialisé de l'entité `Operation`.
//!
//! ## Gestion d'erreurs
//! Pattern at-most-once : si Kafka est down, l'opération VCS est quand
//! même persistée en base. L'événement est perdu, pas l'opération.

use std::time::Duration;

use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use tracing::{info, warn};

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::event_publisher::EventPublisher;

/// Producteur d'événements Kafka (Nen).
///
/// Encapsule un `FutureProducer` rdkafka configuré pour le cluster SHINOBI.
/// Thread-safe (`Send + Sync`) par conception — le `FutureProducer` est
/// conçu pour être partagé via `Arc`.
pub struct KafkaEventPublisher {
    producer: FutureProducer,
    topic: String,
}

impl KafkaEventPublisher {
    /// Crée un nouveau publisher Kafka.
    ///
    /// # Arguments
    /// - `brokers` : liste de brokers Kafka (ex: `"localhost:9092"`)
    /// - `topic` : nom du topic cible (ex: `"shinobi.vcs.operations"`)
    ///
    /// # Errors
    /// Retourne une erreur si la configuration rdkafka est invalide.
    /// **Ne vérifie PAS** la connectivité au cluster — la connexion
    /// est lazy (premier message envoyé).
    pub fn new(brokers: &str, topic: &str) -> Result<Self, DomainError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.ms", "100")
            .create()
            .map_err(|e| {
                DomainError::Internal(format!("Kafka producer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            "KafkaEventPublisher initialisé (connexion lazy)"
        );

        Ok(Self {
            producer,
            topic: topic.to_string(),
        })
    }
}

#[async_trait]
impl EventPublisher for KafkaEventPublisher {
    async fn publish_operation_created(
        &self,
        operation: &Operation,
    ) -> Result<(), DomainError> {
        // Sérialiser l'opération en JSON
        let payload = serde_json::to_string(operation).map_err(|e| {
            DomainError::Internal(format!("JSON serialization failed: {e}"))
        })?;

        // Clé = operation.id → partitionnement déterministe
        let key = operation.id.to_string();

        // Publier sur Kafka avec timeout de 5 secondes
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
                    operation_id = %operation.id,
                    "✅ Événement operation_created publié sur Kafka"
                );
                Ok(())
            }
            Err((kafka_error, _)) => {
                // Log l'erreur mais NE PAS paniquer — at-most-once
                warn!(
                    topic = %self.topic,
                    operation_id = %operation.id,
                    error = %kafka_error,
                    "⚠️ Échec publication Kafka — opération persistée mais événement perdu"
                );
                Err(DomainError::Internal(format!(
                    "Kafka publish failed: {kafka_error}"
                )))
            }
        }
    }
}
