//! Adaptateur Nen — Publication événementielle via Apache Kafka.
//!
//! Utilise `rdkafka` (wrapper librdkafka) pour publier des événements
//! JSON sur les topics SHINOBI.
//!
//! ## Topics
//! - `shinobi.vcs.operations` : opérations VCS créées
//! - `shinobi.tensai.analysis-complete` : analyse sémantique terminée
//! - `shinobi.events.webhooks` : événements webhook (Phase 34-V2 — pont Nen→Chakra)
//! - `shinobi.jutsu.pipeline` : événements pipeline CI/CD (Phase 40 — Jutsu Runner)
//!
//! ## Architecture
//! - `FutureProducer` : producteur async natif rdkafka (compatible tokio).
//! - Clé de partitionnement : `operation.id` (UUID) — garantit que
//!   toutes les mutations d'une opération arrivent sur la même partition.
//! - Payload : JSON sérialisé.
//!
//! ## Phase 34-V2 — Pont Nen→Chakra
//! Le `KafkaEventPublisher` encapsule un `ChakraProducer` optionnel.
//! Quand un Use Case appelle `publish_webhook_event()`, l'événement est
//! publié sur le topic Chakra via le même producteur Kafka.
//!
//! ## Gestion d'erreurs
//! Pattern at-most-once : si Kafka est down, l'opération VCS est quand
//! même persistée en base. L'événement est perdu, pas l'opération.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use tracing::{debug, info, warn};
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::entities::webhook::WebhookEvent;
use domain::errors::DomainError;
use domain::ports::event_publisher::{AnalysisCompleteSummary, EventPublisher};

use super::chakra_producer::ChakraProducer;

/// Producteur d'événements Kafka (Nen).
///
/// Encapsule un `FutureProducer` rdkafka configuré pour le cluster SHINOBI,
/// plus un `ChakraProducer` optionnel pour le pont Nen→Chakra (Phase 34-V2).
///
/// Thread-safe (`Send + Sync`) par conception — le `FutureProducer` est
/// conçu pour être partagé via `Arc`.
pub struct KafkaEventPublisher {
    producer: FutureProducer,
    topic: String,
    /// Topic dédié pour les événements d'analyse (Phase 7B).
    /// Séparé de `topic` pour éviter la boucle infinie du consumer Tensai.
    analysis_topic: String,
    /// Producteur Chakra optionnel — pont vers le topic webhook (Phase 34-V2).
    /// `None` si le système Chakra est désactivé (`CHAKRA_ENABLED=false`).
    chakra_producer: Option<Arc<ChakraProducer>>,
    /// Topic Jutsu pour les événements pipeline CI/CD (Phase 40).
    /// Ex: `"shinobi.jutsu.pipeline"`
    jutsu_topic: String,
    /// Topic dédié Kage Bunshin — file d'auto-healing séparée (Phase 41).
    /// Ex: `"shinobi.jutsu.kage-bunshin"`
    /// Vegapunk Tweak #10 : ne pas bloquer les workers pipeline.
    kage_bunshin_topic: String,
}

impl KafkaEventPublisher {
    /// Crée un nouveau publisher Kafka.
    ///
    /// # Arguments
    /// - `brokers` : liste de brokers Kafka (ex: `"localhost:9092"`)
    /// - `topic` : topic opérations VCS (ex: `"shinobi.vcs.operations"`)
    /// - `analysis_topic` : topic analyse terminée (ex: `"shinobi.tensai.analysis-complete"`)
    /// - `chakra_producer` : producteur Chakra optionnel (Phase 34-V2)
    /// - `jutsu_topic` : topic pipeline CI/CD (Phase 40)
    ///
    /// # Errors
    /// Retourne une erreur si la configuration rdkafka est invalide.
    /// **Ne vérifie PAS** la connectivité au cluster — la connexion
    /// est lazy (premier message envoyé).
    pub fn new(
        brokers: &str,
        topic: &str,
        analysis_topic: &str,
        chakra_producer: Option<Arc<ChakraProducer>>,
        jutsu_topic: &str,
        kage_bunshin_topic: &str,
    ) -> Result<Self, DomainError> {
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
            chakra_bridge = chakra_producer.is_some(),
            "KafkaEventPublisher initialisé (connexion lazy{})",
            if chakra_producer.is_some() { " + pont Chakra" } else { "" }
        );

        Ok(Self {
            producer,
            topic: topic.to_string(),
            analysis_topic: analysis_topic.to_string(),
            chakra_producer,
            jutsu_topic: jutsu_topic.to_string(),
            kage_bunshin_topic: kage_bunshin_topic.to_string(),
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

    /// Phase 34-V2 — Pont Nen→Chakra.
    ///
    /// Délègue au `ChakraProducer` pour publier sur le topic webhook.
    /// Si Chakra est désactivé (`chakra_producer: None`), retourne `Ok(())`
    /// silencieusement (graceful degradation).
    async fn publish_webhook_event(
        &self,
        event: &WebhookEvent,
    ) -> Result<(), DomainError> {
        match &self.chakra_producer {
            Some(producer) => producer.publish(event).await,
            None => {
                debug!(
                    event_id = %event.id,
                    event_type = %event.event_type.as_sql_str(),
                    "Chakra désactivé — événement webhook ignoré (graceful degradation)"
                );
                Ok(())
            }
        }
    }

    /// Phase 40 — Publication pipeline requested pour le Jutsu Runner.
    ///
    /// Publie un message JSON sur le topic `shinobi.jutsu.pipeline`
    /// avec les informations nécessaires au JutsuConsumer.
    ///
    /// Phase 40-E-Fix : inclut `pipeline_id` si le pipeline a été
    /// pré-créé par le trigger endpoint (manual trigger).
    async fn publish_pipeline_requested(
        &self,
        repository_id: Uuid,
        commit_id: &str,
        trigger_event: &str,
        pipeline_id: Option<Uuid>,
    ) -> Result<(), DomainError> {
        let mut payload = serde_json::json!({
            "repository_id": repository_id.to_string(),
            "commit_id": commit_id,
            "trigger_event": trigger_event,
        });

        // Phase 40-E-Fix : injecter pipeline_id si fourni (manual trigger)
        if let Some(pid) = pipeline_id {
            payload["pipeline_id"] = serde_json::Value::String(pid.to_string());
        }

        let payload_str = serde_json::to_string(&payload).map_err(|e| {
            DomainError::Internal(format!("JSON serialization failed: {e}"))
        })?;

        let key = repository_id.to_string();

        let delivery_result = self
            .producer
            .send(
                FutureRecord::to(&self.jutsu_topic)
                    .key(&key)
                    .payload(&payload_str),
                Duration::from_secs(5),
            )
            .await;

        match delivery_result {
            Ok(delivery) => {
                info!(
                    topic = %self.jutsu_topic,
                    partition = delivery.partition,
                    offset = delivery.offset,
                    repository_id = %repository_id,
                    commit_id = %commit_id,
                    trigger = %trigger_event,
                    pipeline_id = ?pipeline_id,
                    "🥷 Événement pipeline_requested publié sur Kafka"
                );
                Ok(())
            }
            Err((kafka_error, _)) => {
                warn!(
                    topic = %self.jutsu_topic,
                    repository_id = %repository_id,
                    error = %kafka_error,
                    "⚠️ Échec publication pipeline_requested — non-fatal"
                );
                Err(DomainError::Internal(format!(
                    "Kafka publish pipeline_requested failed: {kafka_error}"
                )))
            }
        }
    }

    /// Phase 41 — Publie une demande Kage Bunshin sur le topic dédié.
    ///
    /// Vegapunk Tweak #10 : file Kafka séparée pour ne pas bloquer
    /// les workers pipeline pendant l'inférence LLM (30-60s).
    async fn publish_kage_bunshin_requested(
        &self,
        pipeline_id: Uuid,
        stage_id: Uuid,
        stage_name: &str,
        stage_image: &str,
        stage_commands: &[String],
        error_logs: &str,
        repository_id: Uuid,
        commit_id: &str,
    ) -> Result<(), DomainError> {
        let payload = serde_json::json!({
            "pipeline_id": pipeline_id.to_string(),
            "stage_id": stage_id.to_string(),
            "stage_name": stage_name,
            "stage_image": stage_image,
            "stage_commands": stage_commands,
            "error_logs": error_logs,
            "repository_id": repository_id.to_string(),
            "commit_id": commit_id,
        });

        let payload_str = serde_json::to_string(&payload).map_err(|e| {
            DomainError::Internal(format!("JSON serialization failed: {e}"))
        })?;

        let key = pipeline_id.to_string();

        let delivery_result = self
            .producer
            .send(
                FutureRecord::to(&self.kage_bunshin_topic)
                    .key(&key)
                    .payload(&payload_str),
                Duration::from_secs(5),
            )
            .await;

        match delivery_result {
            Ok(delivery) => {
                info!(
                    topic = %self.kage_bunshin_topic,
                    partition = delivery.partition,
                    offset = delivery.offset,
                    pipeline_id = %pipeline_id,
                    stage = %stage_name,
                    "🥷 Kage Bunshin requested — publié sur file dédiée"
                );
                Ok(())
            }
            Err((kafka_error, _)) => {
                warn!(
                    topic = %self.kage_bunshin_topic,
                    pipeline_id = %pipeline_id,
                    error = %kafka_error,
                    "⚠️ Échec publication kage_bunshin — non-fatal"
                );
                Err(DomainError::Internal(format!(
                    "Kafka publish kage_bunshin failed: {kafka_error}"
                )))
            }
        }
    }
}
