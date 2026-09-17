//! Use Case : Émission d'événements webhook — Phase 34 (Chakra チャクラ)
//!
//! Service utilitaire fire-and-forget appelé depuis les use cases existants
//! (create_operation, create_mr, merge_mr, create_issue, etc.).
//!
//! ## Pattern
//! L'émission est toujours async et non-bloquante :
//! 1. Le use case métier fait son travail (persist DB, etc.)
//! 2. Il appelle `emit()` qui publie sur Kafka en ~2ms
//! 3. Le consumer Chakra traite l'événement de manière asynchrone
//!
//! Si Kafka est down, l'événement est perdu (at-most-once) mais
//! l'opération métier n'est pas impactée.

use std::sync::Arc;

use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::webhook::{WebhookEvent, WebhookEventType};

use infrastructure::events::chakra_producer::ChakraProducer;

/// Service d'émission d'événements webhook.
///
/// Encapsule le producteur Kafka et fournit une API simple
/// pour les use cases métier.
pub struct EmitWebhookEventUseCase {
    producer: Arc<ChakraProducer>,
}

impl EmitWebhookEventUseCase {
    /// Construit le service d'émission.
    pub fn new(producer: Arc<ChakraProducer>) -> Self {
        Self { producer }
    }

    /// Émet un événement webhook de manière fire-and-forget.
    ///
    /// Ne bloque jamais le thread principal. Si Kafka est down,
    /// l'événement est logué et perdu.
    ///
    /// # Arguments
    /// - `event_type` : type d'événement (push, mr_created, etc.)
    /// - `repository_id` : dépôt source
    /// - `actor_id` : acteur ayant déclenché l'événement
    /// - `payload` : payload JSON spécifique au type d'événement
    pub async fn emit(
        &self,
        event_type: WebhookEventType,
        repository_id: Uuid,
        actor_id: Uuid,
        payload: serde_json::Value,
    ) {
        let event = WebhookEvent::new(event_type, repository_id, actor_id, payload);

        info!(
            event_id = %event.id,
            event_type = %event.event_type,
            repository_id = %repository_id,
            "🔔 Chakra — Émission webhook"
        );

        if let Err(e) = self.producer.publish(&event).await {
            warn!(
                event_id = %event.id,
                error = %e,
                "⚠️ Chakra — Échec émission webhook (non-fatal)"
            );
        }
    }
}
