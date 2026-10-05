//! Adaptateur Nen — Consumer Kafka pour l'agent Oracle.
//!
//! Écoute le topic `shinobi.tensai.analysis-complete` et extrait
//! l'`operation_id` de chaque `AnalysisCompleteSummary` pour déclencher
//! la code review par le LLM local.
//!
//! ## Architecture Découplée (Phase 22)
//!
//! Le consumer utilise un pattern **mpsc bounded channel** pour découpler
//! la boucle de polling Kafka du traitement LLM (potentiellement lent).
//!
//! ```text
//! ┌─────────────────────┐     ┌──────────────────────────┐
//! │  Consumer (rapide)   │     │  Worker (séquentiel)      │
//! │  Kafka recv()        │     │  rx.recv()                │
//! │  → tx.send(op_id)   ├────►│  → handler(op_id).await   │
//! │  → offset validé ✅  │ mpsc│  → un par un              │
//! │  → retour écouter    │     │  → pas d'OOM Ollama       │
//! └─────────────────────┘     └──────────────────────────┘
//! ```
//!
//! **Pourquoi ?** Granite3 peut prendre 6+ minutes pour une inférence.
//! Sans découplage, Kafka expulse le consumer après `max.poll.interval.ms`
//! (PollExceeded). Avec le channel, le consumer continue de poller
//! pendant que le worker traite à son rythme.
//!
//! ## Différence avec KafkaEventConsumer
//! Le consumer Tensai désérialise des `Operation` complètes.
//! Le consumer Oracle désérialise des `AnalysisCompleteSummary` (plus léger)
//! et n'a besoin que de l'`operation_id`.

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::Message;
use metrics::counter;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use domain::errors::DomainError;

/// Callback invoqué pour chaque operation_id reçu du topic analysis-complete.
pub type OracleHandler =
    Box<dyn Fn(Uuid) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

/// Taille du buffer mpsc — salle d'attente interne.
/// 32 messages max en file d'attente avant que le consumer ralentisse
/// (backpressure naturelle). Suffisant pour un bulk import de ~30 commits
/// sans bloquer le consumer.
const CHANNEL_BUFFER_SIZE: usize = 32;

/// Consumer Kafka pour l'agent Oracle Reviewer.
///
/// Écoute le topic `shinobi.tensai.analysis-complete` et transmet
/// l'`operation_id` au worker via un canal mpsc borné.
pub struct OracleKafkaConsumer {
    consumer: StreamConsumer,
    topic: String,
    cancel_token: CancellationToken,
}

/// Payload léger du topic analysis-complete.
/// On n'a besoin que de l'operation_id (+ repository_id pour Phase 10A).
#[derive(serde::Deserialize)]
struct AnalysisSummaryPayload {
    operation_id: Uuid,
    #[allow(dead_code)] // Phase 10A — sera consommé quand Oracle devient repo-aware
    repository_id: Option<Uuid>,
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
            // Phase 22 : Filet de sécurité — 15 min max entre polls.
            // Le découplage mpsc rend ceci quasi-impossible à atteindre,
            // mais c'est une défense en profondeur au cas où le channel
            // serait plein ET le send() bloquerait longtemps.
            .set("max.poll.interval.ms", "900000")
            .create()
            .map_err(|e| {
                DomainError::Internal(format!("Oracle Kafka consumer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group_id = %group_id,
            "OracleKafkaConsumer initialisé (mpsc découplé)"
        );

        Ok(Self {
            consumer,
            topic: topic.to_string(),
            cancel_token,
        })
    }

    /// Démarre la boucle de consommation Oracle avec découplage mpsc.
    ///
    /// Spawne un **worker séquentiel** qui traite les messages un par un,
    /// pendant que la boucle principale continue de poller Kafka sans blocage.
    pub async fn start(&self, handler: OracleHandler) -> Result<(), DomainError> {
        self.consumer
            .subscribe(&[&self.topic])
            .map_err(|e| {
                DomainError::Internal(format!(
                    "Oracle Kafka subscribe failed on topic '{}': {e}",
                    self.topic
                ))
            })?;

        // ── Canal mpsc borné : la "salle d'attente" ──────────────
        let (tx, mut rx) = mpsc::channel::<Uuid>(CHANNEL_BUFFER_SIZE);

        // ── Worker séquentiel : traite les reviews une par une ───
        let worker_cancel = self.cancel_token.clone();
        let topic_clone = self.topic.clone();
        let worker_handle = tokio::spawn(async move {
            info!("🔮 Oracle Worker — Démarré (traitement séquentiel)");

            loop {
                tokio::select! {
                    _ = worker_cancel.cancelled() => {
                        info!("🛑 Oracle Worker — Shutdown gracieux");
                        break;
                    }
                    maybe_op_id = rx.recv() => {
                        match maybe_op_id {
                            Some(operation_id) => {
                                info!(
                                    operation_id = %operation_id,
                                    "🔮 Oracle Worker — Traitement démarré"
                                );
                                handler(operation_id).await;
                                counter!(
                                    "oracle_reviews_processed_total",
                                    "topic" => topic_clone.clone()
                                ).increment(1);
                            }
                            None => {
                                // Le sender (consumer) a été droppé → fin normale.
                                info!("🔮 Oracle Worker — Canal fermé, arrêt");
                                break;
                            }
                        }
                    }
                }
            }

            info!("🔮 Oracle Worker — Arrêté proprement");
        });

        info!(
            topic = %self.topic,
            buffer_size = CHANNEL_BUFFER_SIZE,
            "🔮 Oracle Consumer — Boucle de consommation démarrée (mpsc découplé)"
        );

        // ── Consumer : polling Kafka rapide → dispatch dans le canal ──
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
                                        "📨 Oracle — Événement analysis-complete reçu → file d'attente"
                                    );

                                    // Métriques Prometheus
                                    counter!("kafka_messages_consumed_total", "consumer" => "oracle", "topic" => self.topic.clone())
                                        .increment(1);

                                    // Envoyer dans le canal mpsc (non bloquant si buffer < 32).
                                    // Si le buffer est plein, on attend — backpressure naturelle.
                                    if let Err(e) = tx.send(summary.operation_id).await {
                                        error!(
                                            operation_id = %e.0,
                                            "❌ Oracle — Canal mpsc fermé, message perdu"
                                        );
                                    }
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

        // ── Cleanup : droper le sender et attendre le worker ────
        drop(tx);
        let _ = worker_handle.await;

        info!("🔮 Oracle Consumer — Boucle arrêtée proprement");
        Ok(())
    }
}
