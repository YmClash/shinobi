//! Port: EventPublisher — Contrat de publication événementielle (Nen).
//!
//! Ce trait abstrait le bus d'événements sous-jacent (Kafka, in-memory, etc.).
//! L'adaptateur concret dans infrastructure/ implémente le transport réel.
//!
//! ## Pattern
//! Fire-and-Forget avec gestion d'erreur : l'appelant est notifié si la
//! publication échoue, mais l'opération VCS n'est PAS annulée (at-most-once).

use async_trait::async_trait;

use crate::entities::operation::Operation;
use crate::errors::DomainError;

/// Contrat de publication d'événements métier.
///
/// Conçu pour le pattern événementiel : chaque mutation métier
/// significative est propagée sur le bus pour les consommateurs
/// downstream (CI/CD, monitoring, IA).
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publie un événement "opération créée" sur le bus.
    ///
    /// Le payload contient l'`Operation` sérialisée en JSON.
    /// La clé du message est l'`operation.id` (partitionnement Kafka).
    async fn publish_operation_created(
        &self,
        operation: &Operation,
    ) -> Result<(), DomainError>;
}
