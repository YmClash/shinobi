//! Adaptateur Nen — Publication événementielle via Apache Kafka.
//!
//! Utilise `rdkafka` (wrapper librdkafka) pour publier des événements
//! JSON sur les topics SHINOBI.
//!
//! ## Topics
//! - `shinobi.vcs.operations` : opérations VCS créées
//! - `shinobi.tensai.analysis-complete` : analyse sémantique terminée
//!
//! ## Architecture
//! - `FutureProducer` : producteur async natif rdkafka (compatible tokio).
//! - Clé de partitionnement : `operation.id` (UUID) — garantit que
//!   toutes les mutations d'une opération arrivent sur la même partition.
//! - Payload : JSON sérialisé.
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
use domain::ports::event_publisher::{AnalysisCompleteSummary, EventPublisher};

/// Producteur d'événements Kafka (Nen).
///
/// Encapsule un `FutureProducer` rdkafka configuré pour le cluster SHINOBI.
/// Thread-safe (`Send + Sync`) par conception — le `FutureProducer` est
/// conçu pour être partagé via `Arc`.
pub struct KafkaEventPublisher {
    producer: FutureProducer,
    topic: String,
    /// Topic dédié pour les événements d'analyse (Phase 7B).
    /// Séparé de `topic` pour éviter la boucle infinie du consumer Tensai.
    analysis_topic: String,
}

impl KafkaEventPublisher {
    /// Crée un nouveau publisher Kafka.
    ///
    /// # Arguments
    /// - `brokers` : liste de brokers Kafka (ex: `"localhost:9092"`)
    /// - `topic` : topic opérations VCS (ex: `"shinobi.vcs.operations"`)
    /// - `analysis_topic` : topic analyse terminée (ex: `"shinobi.tensai.analysis-complete"`)
    ///
    /// # Errors
    /// Retourne une erreur si la configuration rdkafka est invalide.
    /// **Ne vérifie PAS** la connectivité au cluster — la connexion
    /// est lazy (premier message envoyé).
    pub fn new(brokers: &str, topic: &str, analysis_topic: &str) -> Result<Self, DomainError> {
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
            analysis_topic = %analysis_topic,
            "KafkaEventPublisher initialisé (connexion lazy)"
        );

        Ok(Self {
            producer,
            topic: topic.to_string(),
            analysis_topic: analysis_topic.to_string(),
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

    async fn publish_analysis_complete(
        &self,
        summary: &AnalysisCompleteSummary,
    ) -> Result<(), DomainError> {
        let payload = serde_json::to_string(summary).map_err(|e| {
            DomainError::Internal(format!("JSON serialization failed: {e}"))
        })?;

        let key = summary.operation_id.to_string();

        let delivery_result = self
            .producer
            .send(
                FutureRecord::to(&self.analysis_topic)
                    .key(&key)
                    .payload(&payload),
                Duration::from_secs(5),
            )
            .await;

        match delivery_result {
            Ok(delivery) => {
                info!(
                    topic = %self.analysis_topic,
                    partition = delivery.partition,
                    offset = delivery.offset,
                    operation_id = %summary.operation_id,
                    total_chunks = summary.total_chunks,
                    "✅ Événement analysis_complete publié sur Kafka"
                );
                Ok(())
            }
            Err((kafka_error, _)) => {
                warn!(
                    topic = %self.analysis_topic,
                    operation_id = %summary.operation_id,
                    error = %kafka_error,
                    "⚠️ Échec publication analysis_complete — non-fatal"
                );
                Err(DomainError::Internal(format!(
                    "Kafka publish analysis_complete failed: {kafka_error}"
                )))
            }
        }
    }
}
