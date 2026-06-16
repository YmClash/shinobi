//! Adaptateur Nen — Consommation événementielle via Apache Kafka.
//!
//! Utilise `rdkafka` (wrapper librdkafka) pour consommer les événements
//! JSON du topic `shinobi.vcs.operations` et les transmettre à l'agent
//! Tensai pour analyse sémantique en temps réel.
//!
//! ## Architecture
//! - `StreamConsumer` : consumer async natif rdkafka (compatible tokio).
//! - Consumer group : `shinobi-tensai-analyzer` — isolé du producer.
//! - Auto-offset reset : `earliest` — ne rien rater au premier démarrage.
//! - Désérialisation JSON du payload → `Operation` (entité domain).
//!
//! ## Shutdown Gracieux
//! Utilise `tokio_util::sync::CancellationToken` pour arrêter la boucle
//! de consommation proprement lors du Ctrl+C.

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::event_consumer::OperationHandler;

/// Consumer Kafka pour l'agent Tensai.
///
/// Écoute le topic `shinobi.vcs.operations` et transmet chaque
/// `Operation` désérialisée au handler fourni (use case d'analyse).
pub struct KafkaEventConsumer {
    consumer: StreamConsumer,
    topic: String,
    cancel_token: CancellationToken,
}

impl KafkaEventConsumer {
    /// Crée un nouveau consumer Kafka.
    ///
    /// # Arguments
    /// - `brokers` : liste de brokers Kafka (ex: `"localhost:9092"`)
    /// - `topic` : topic à écouter (ex: `"shinobi.vcs.operations"`)
    /// - `group_id` : consumer group (ex: `"shinobi-tensai-analyzer"`)
    /// - `cancel_token` : token pour le shutdown gracieux
    ///
    /// # Errors
    /// Retourne une erreur si la configuration rdkafka est invalide.
    pub fn new(
        brokers: &str,
        topic: &str,
        group_id: &str,
        cancel_token: CancellationToken,
    ) -> Result<Self, DomainError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "true")
            .set("auto.commit.interval.ms", "5000")
            .set("session.timeout.ms", "30000")
            .create()
            .map_err(|e| {
                DomainError::Internal(format!("Kafka consumer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group_id = %group_id,
            "KafkaEventConsumer initialisé"
        );

        Ok(Self {
            consumer,
            topic: topic.to_string(),
            cancel_token,
        })
    }

    /// Démarre la boucle de consommation.
    ///
    /// Écoute le topic Kafka et transmet chaque message au handler.
    /// La boucle tourne indéfiniment jusqu'au déclenchement du
    /// `CancellationToken` (shutdown gracieux).
    ///
    /// # Arguments
    /// - `handler` : callback invoqué pour chaque `Operation` reçue.
    pub async fn start(&self, handler: OperationHandler) -> Result<(), DomainError> {
        // S'abonner au topic.
        self.consumer
            .subscribe(&[&self.topic])
            .map_err(|e| {
                DomainError::Internal(format!(
                    "Kafka subscribe failed on topic '{}': {e}",
                    self.topic
                ))
            })?;

        info!(
            topic = %self.topic,
            "🧠 Tensai Consumer — Boucle de consommation démarrée"
        );

        // Boucle de consommation avec shutdown gracieux.
        loop {
            tokio::select! {
                // Shutdown gracieux : le token est annulé.
                _ = self.cancel_token.cancelled() => {
                    info!("🛑 Tensai Consumer — Shutdown gracieux déclenché");
                    break;
                }
                // Message reçu du topic.
                message_result = self.consumer.recv() => {
                    match message_result {
                        Ok(message) => {
                            // Extraire le payload JSON.
                            let payload = match message.payload_view::<str>() {
                                Some(Ok(text)) => text,
                                Some(Err(e)) => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        error = %e,
                                        "⚠️ Tensai — Payload non-UTF8, skip"
                                    );
                                    continue;
                                }
                                None => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        "⚠️ Tensai — Message sans payload, skip"
                                    );
                                    continue;
                                }
                            };

                            // Désérialiser l'Operation.
                            match serde_json::from_str::<Operation>(payload) {
                                Ok(operation) => {
                                    info!(
                                        operation_id = %operation.id,
                                        author_id = %operation.author_id,
                                        has_ipfs = operation.has_ipfs_content(),
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        "📨 Tensai — Opération reçue de Kafka"
                                    );

                                    // Invoquer le handler (analyse sémantique).
                                    handler(operation).await;
                                }
                                Err(e) => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        error = %e,
                                        "⚠️ Tensai — Désérialisation Operation échouée"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                "❌ Tensai — Erreur Kafka recv()"
                            );
                            // Petite pause avant de réessayer pour éviter un busy-loop.
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }

        info!("🧠 Tensai Consumer — Boucle arrêtée proprement");
        Ok(())
    }
}
