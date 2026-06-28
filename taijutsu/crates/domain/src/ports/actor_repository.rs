//! Port: ActorRepository — Contrat de persistence des acteurs.
//!
//! Ce trait définit le contrat pour sauvegarder et retrouver des acteurs
//! (humains, agents IA, système). L'implémentation concrète vit dans
//! infrastructure/ (Fūinjutsu / PostgreSQL).

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::actor::Actor;
use crate::errors::DomainError;

/// Contrat de persistence pour les acteurs de la Forge Sociale.
///
/// Implémenté par `PostgresActorRepository` dans la couche infrastructure.
#[async_trait]
pub trait ActorRepository: Send + Sync {
    /// Persiste un nouvel acteur.
    async fn save(&self, actor: &Actor) -> Result<(), DomainError>;

    /// Retrouve un acteur par son identifiant.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Actor>, DomainError>;

    /// Retrouve un acteur par son handle unique.
    /// Le handle est unique tous types confondus (namespace universel).
    async fn find_by_handle(&self, handle: &str) -> Result<Option<Actor>, DomainError>;
}
