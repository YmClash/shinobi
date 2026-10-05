//! Adaptateur Nen — Consommation événementielle via Apache Kafka.
//!
//! Utilise `rdkafka` (wrapper librdkafka) pour consommer les événements
//! JSON du topic `shinobi.vcs.operations` et les transmettre à l'agent
//! Tensai pour analyse sémantique en temps réel.
//!
//! ## Architecture Découplée (Phase 22)
//!
//! Le consumer utilise un pattern **mpsc bounded channel** pour découpler
//! la boucle de polling Kafka du traitement sémantique (embedding + chunking).
//!
//! ```text
//! ┌─────────────────────┐     ┌──────────────────────────┐
//! │  Consumer (rapide)   │     │  Worker (séquentiel)      │
//! │  Kafka recv()        │     │  rx.recv()                │
//! │  → tx.send(op)      ├────►│  → handler(op).await      │
//! │  → offset validé ✅  │ mpsc│  → un par un              │
//! │  → retour écouter    │     │  → pas d'OOM embedding    │
//! └─────────────────────┘     └──────────────────────────┘
//! ```
//!
//! **Pourquoi ?** Lors d'un bulk import (5+ commits), sans découplage,
//! le consumer traite chaque message séquentiellement dans la boucle recv(),
//! ce qui peut dépasser le `max.poll.interval.ms` de Kafka et causer
//! un PollExceeded. Avec le channel, le consumer reste réactif.
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
use metrics::counter;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::event_consumer::OperationHandler;

/// Taille du buffer mpsc — salle d'attente interne.
/// 32 messages max en file d'attente avant que le consumer ralentisse
/// (backpressure naturelle). Suffisant pour un bulk import sans blocage.
const CHANNEL_BUFFER_SIZE: usize = 32;

/// Consumer Kafka pour l'agent Tensai.
///
/// Écoute le topic `shinobi.vcs.operations` et transmet chaque
/// `Operation` désérialisée au worker via un canal mpsc borné.
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
            // Phase 22 : Filet de sécurité — 15 min max entre polls.
            // Le découplage mpsc rend ceci quasi-impossible à atteindre,
            // mais c'est une défense en profondeur.
            .set("max.poll.interval.ms", "900000")
            .create()
            .map_err(|e| {
                DomainError::Internal(format!("Kafka consumer creation failed: {e}"))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group_id = %group_id,
            "KafkaEventConsumer initialisé (mpsc découplé)"
        );

        Ok(Self {
            consumer,
            topic: topic.to_string(),
            cancel_token,
        })
    }

    /// Démarre la boucle de consommation avec découplage mpsc.
    ///
    /// Spawne un **worker séquentiel** qui traite les messages un par un,
    /// pendant que la boucle principale continue de poller Kafka sans blocage.
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

        // ── Canal mpsc borné : la "salle d'attente" ──────────────
        let (tx, mut rx) = mpsc::channel::<Operation>(CHANNEL_BUFFER_SIZE);

        // ── Worker séquentiel : traite les analyses une par une ──
        let worker_cancel = self.cancel_token.clone();
        let topic_clone = self.topic.clone();
        let worker_handle = tokio::spawn(async move {
            info!("🧠 Tensai Worker — Démarré (traitement séquentiel)");

            loop {
                tokio::select! {
                    _ = worker_cancel.cancelled() => {
                        info!("🛑 Tensai Worker — Shutdown gracieux");
                        break;
                    }
                    maybe_operation = rx.recv() => {
                        match maybe_operation {
                            Some(operation) => {
                                info!(
                                    operation_id = %operation.id,
                                    "🧠 Tensai Worker — Traitement démarré"
                                );
                                handler(operation).await;
                                counter!(
                                    "tensai_analyses_processed_total",
                                    "topic" => topic_clone.clone()
                                ).increment(1);
                            }
                            None => {
                                // Le sender (consumer) a été droppé → fin normale.
                                info!("🧠 Tensai Worker — Canal fermé, arrêt");
                                break;
                            }
                        }
                    }
                }
            }

            info!("🧠 Tensai Worker — Arrêté proprement");
        });

        info!(
            topic = %self.topic,
            buffer_size = CHANNEL_BUFFER_SIZE,
            "🧠 Tensai Consumer — Boucle de consommation démarrée (mpsc découplé)"
        );

        // ── Consumer : polling Kafka rapide → dispatch dans le canal ──
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
                                        "📨 Tensai — Opération reçue de Kafka → file d'attente"
                                    );

                                    // Métriques Prometheus
                                    counter!("kafka_messages_consumed_total", "consumer" => "tensai", "topic" => self.topic.clone())
                                        .increment(1);

                                    // Envoyer dans le canal mpsc (non bloquant si buffer < 32).
                                    // Si le buffer est plein, on attend — backpressure naturelle.
                                    if let Err(e) = tx.send(operation).await {
                                        error!(
                                            operation_id = %e.0.id,
                                            "❌ Tensai — Canal mpsc fermé, message perdu"
                                        );
                                    }
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
                            counter!("kafka_consumer_errors_total", "consumer" => "tensai")
                                .increment(1);
                            // Petite pause avant de réessayer pour éviter un busy-loop.
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }

        // ── Cleanup : droper le sender et attendre le worker ────
        drop(tx);
        let _ = worker_handle.await;

        info!("🧠 Tensai Consumer — Boucle arrêtée proprement");
        Ok(())
    }
}
