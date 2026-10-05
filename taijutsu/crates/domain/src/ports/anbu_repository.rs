//! Port: AnbuRepository — Contrat de persistance des checkpoints ANBU.
//!
//! Implémenté par l'adaptateur PostgreSQL dans infrastructure/.

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::anbu_checkpoint::AnbuCheckpoint;
use crate::errors::DomainError;

/// Contrat de persistance pour les checkpoints ANBU.
#[async_trait]
pub trait AnbuRepository: Send + Sync {
    /// Insère un nouveau checkpoint ANBU.
    async fn save_checkpoint(&self, checkpoint: &AnbuCheckpoint) -> Result<(), DomainError>;

    /// Liste les checkpoints d'un dépôt (par date décroissante).
    async fn list_checkpoints(
        &self,
        repository_id: &Uuid,
        limit: usize,
    ) -> Result<Vec<AnbuCheckpoint>, DomainError>;

    /// Trouve un checkpoint par son ID.
    async fn find_checkpoint(&self, id: &Uuid) -> Result<Option<AnbuCheckpoint>, DomainError>;
}
