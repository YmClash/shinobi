//! Helper d'émission webhook — Phase 34-V2 (Le Pont Nen→Chakra)
//!
//! Fournit une fonction fire-and-forget réutilisable par tous les Use Cases.
//! Encapsule la logique `if let Some(...) { tokio::spawn(...) }` pour éviter
//! la duplication dans chaque Use Case (DRY).
//!
//! ## Contrat
//! - Si `event_publisher` est `None` (Kafka/Chakra désactivé), ne fait rien.
//! - Si la publication Kafka échoue, log un `warn!` — l'opération métier n'est
//!   **jamais** impactée (at-most-once, graceful degradation).
//! - L'émission est non-bloquante : `tokio::spawn` détache un future indépendant.
//!
//! ## Utilisation
//! ```rust,ignore
//! // Dans un Use Case, après la logique métier :
//! webhook_emit::emit_webhook_fire_and_forget(
//!     &self.event_publisher,
//!     WebhookEventType::MrCreated,
//!     repository_id,
//!     actor_id,
//!     serde_json::json!({ "action": "opened", "number": mr.number }),
//! );
//! ```

use std::sync::Arc;

use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::webhook::{WebhookEvent, WebhookEventType};
use domain::ports::event_publisher::EventPublisher;

/// Émet un événement webhook de manière fire-and-forget via le bus Kafka.
///
/// # Arguments
/// - `event_publisher` : port `EventPublisher` optionnel (injecté dans le Use Case)
/// - `event_type` : type d'événement webhook (`Push`, `MrCreated`, etc.)
/// - `repository_id` : UUID du dépôt source
/// - `actor_id` : UUID de l'acteur déclencheur
/// - `payload` : contenu JSON standardisé (compatible GitHub/Gitea)
///
/// # Sémantique
/// - **Fire-and-forget** : la fonction retourne immédiatement. Le `tokio::spawn`
///   détache un future qui publie sur Kafka en arrière-plan.
/// - **At-most-once** : si Kafka est down, l'événement est perdu — l'opération
///   métier (MR créée, issue ouverte, etc.) n'est pas impactée.
/// - **Graceful degradation** : si `event_publisher` est `None`, la fonction
///   est un no-op complet (pas de log, pas d'allocation).
pub fn emit_webhook_fire_and_forget(
    event_publisher: &Option<Arc<dyn EventPublisher>>,
    event_type: WebhookEventType,
    repository_id: Uuid,
    actor_id: Uuid,
    payload: serde_json::Value,
) {
    if let Some(publisher) = event_publisher.clone() {
        let event = WebhookEvent::new(event_type, repository_id, actor_id, payload);
        let event_id = event.id;
        let event_type_label = event.event_type.as_sql_str();

        tokio::spawn(async move {
            info!(
                event_id = %event_id,
                event_type = %event_type_label,
                repository_id = %repository_id,
                actor_id = %actor_id,
                "🔔 Phase 34-V2 — Émission webhook → Kafka (fire-and-forget)"
            );
            if let Err(e) = publisher.publish_webhook_event(&event).await {
                warn!(
                    event_id = %event_id,
                    event_type = %event_type_label,
                    error = %e,
                    "⚠️ Phase 34-V2 — Échec émission webhook (non-fatal, at-most-once)"
                );
            }
        });
    }
}
