//! Port: EventConsumer — Contrat de consommation événementielle.
//!
//! Ce trait abstrait le bus de consommation sous-jacent (Kafka, in-memory, etc.).
//! L'adaptateur concret dans infrastructure/ implémente la boucle de lecture réelle.
//!
//! ## Pattern
//! Boucle infinie avec shutdown gracieux : le consumer lit les messages
//! du topic et les transmet au handler fourni. L'arrêt est déclenché
//! par le `CancellationToken`.
//!
//! ## Relation avec EventPublisher
//! - `EventPublisher` : produit des événements (côté écriture).
//! - `EventConsumer` : consomme des événements (côté lecture / agent IA).

use crate::entities::operation::Operation;
use crate::errors::DomainError;

/// Callback invoqué pour chaque opération reçue du bus événementiel.
///
/// Le consumer désérialise le message et appelle ce handler
/// avec l'`Operation` reconstituée. Le handler est responsable
/// du traitement métier (analyse sémantique, etc.).
pub type OperationHandler =
    Box<dyn Fn(Operation) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

/// Contrat de consommation d'événements métier.
///
/// Conçu pour le pattern agent IA : chaque mutation VCS publiée
/// sur le bus est interceptée par un ou plusieurs consumers
/// pour analyse, indexation, ou déclenchement de pipelines.
pub trait EventConsumer: Send + Sync {
    /// Démarre la boucle de consommation (bloquante).
    ///
    /// Cette méthode ne retourne que lorsque le consumer est arrêté
    /// (shutdown gracieux via `CancellationToken` ou erreur fatale).
    ///
    /// # Arguments
    /// - `handler` : callback invoqué pour chaque `Operation` reçue.
    ///
    /// # Errors
    /// Retourne une erreur si la connexion au bus échoue ou si
    /// une erreur fatale interrompt la boucle de consommation.
    fn start(
        &self,
        handler: OperationHandler,
    ) -> impl std::future::Future<Output = Result<(), DomainError>> + Send;
}
