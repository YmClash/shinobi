//! Port: OperationRepository — Contrat de persistence des opérations.
//!
//! Ce trait définit le contrat pour sauvegarder et retrouver des opérations
//! VCS. L'implémentation concrète vit dans infrastructure/ (Fūinjutsu / PostgreSQL).

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::operation::Operation;
use crate::errors::DomainError;

/// Contrat de persistence pour les opérations VCS.
///
/// Implémenté par `PostgresOperationRepository` dans la couche infrastructure.
#[async_trait]
pub trait OperationRepository: Send + Sync {
    /// Persiste une nouvelle opération.
    async fn save(&self, operation: &Operation) -> Result<(), DomainError>;

    /// Retrouve une opération par son identifiant.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Operation>, DomainError>;

    /// Liste les opérations les plus récentes, ordonnées par date décroissante.
    async fn list_recent(&self, limit: usize) -> Result<Vec<Operation>, DomainError>;

    /// Retrouve toutes les opérations d'un auteur donné.
    async fn find_by_author(&self, author_id: &Uuid) -> Result<Vec<Operation>, DomainError>;
}
