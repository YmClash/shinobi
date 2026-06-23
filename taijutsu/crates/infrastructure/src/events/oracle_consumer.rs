//! Adaptateur Nen — Consumer Kafka pour l'agent Oracle.
//!
//! Écoute le topic `shinobi.tensai.analysis-complete` et extrait
//! l'`operation_id` de chaque `AnalysisCompleteSummary` pour déclencher
//! la code review par le LLM local.
//!
//! ## Différence avec KafkaEventConsumer
//! Le consumer Tensai désérialise des `Operation` complètes.
//! Le consumer Oracle désérialise des `AnalysisCompleteSummary` (plus léger)
//! et n'a besoin que de l'`operation_id`.

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use metrics::counter;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use domain::errors::DomainError;

/// Callback invoqué pour chaque operation_id reçu du topic analysis-complete.
pub type OracleHandler =
    Box<dyn Fn(Uuid) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

/// Consumer Kafka pour l'agent Oracle Reviewer.
///
/// Écoute le topic `shinobi.tensai.analysis-complete` et transmet
/// l'`operation_id` au handler fourni (use case de review).
pub struct OracleKafkaConsumer {
    consumer: StreamConsumer,
    topic: String,
    cancel_token: CancellationToken,
}

/// Payload léger du topic analysis-complete.
/// On n'a besoin que de l'operation_id.
#[derive(serde::Deserialize)]
struct AnalysisSummaryPayload {
    operation_id: Uuid,
}

impl OracleKafkaConsumer {
    /// Crée un nouveau consumer Oracle.
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
                DomainError::Internal(format!("Oracle Kafka consumer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group_id = %group_id,
            "OracleKafkaConsumer initialisé"
        );

        Ok(Self {
            consumer,
            topic: topic.to_string(),
            cancel_token,
        })
    }

    /// Démarre la boucle de consommation Oracle.
    pub async fn start(&self, handler: OracleHandler) -> Result<(), DomainError> {
        self.consumer
            .subscribe(&[&self.topic])
            .map_err(|e| {
                DomainError::Internal(format!(
                    "Oracle Kafka subscribe failed on topic '{}': {e}",
                    self.topic
                ))
            })?;

        info!(
            topic = %self.topic,
            "🔮 Oracle Consumer — Boucle de consommation démarrée"
        );

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("🛑 Oracle Consumer — Shutdown gracieux déclenché");
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
                                        "⚠️ Oracle — Payload non-UTF8, skip"
                                    );
                                    continue;
                                }
                                None => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        "⚠️ Oracle — Message sans payload, skip"
                                    );
                                    continue;
                                }
                            };

                            // Désérialiser l'AnalysisCompleteSummary → extraire operation_id.
                            match serde_json::from_str::<AnalysisSummaryPayload>(payload) {
                                Ok(summary) => {
                                    info!(
                                        operation_id = %summary.operation_id,
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        "📨 Oracle — Événement analysis-complete reçu"
                                    );

                                    // Métriques Prometheus
                                    counter!("kafka_messages_consumed_total", "consumer" => "oracle", "topic" => self.topic.clone())
                                        .increment(1);

                                    handler(summary.operation_id).await;
                                }
                                Err(e) => {
                                    warn!(
                                        partition = message.partition(),
                                        offset = message.offset(),
                                        error = %e,
                                        "⚠️ Oracle — Désérialisation AnalysisCompleteSummary échouée"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                "❌ Oracle — Erreur Kafka recv()"
                            );
                            counter!("kafka_consumer_errors_total", "consumer" => "oracle")
                                .increment(1);
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }

        info!("🔮 Oracle Consumer — Boucle arrêtée proprement");
        Ok(())
    }
}
