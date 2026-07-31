//! Use case : ListCheckpoints — Liste les checkpoints ANBU d'un dépôt.

use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use domain::entities::anbu_checkpoint::AnbuCheckpoint;
use domain::errors::DomainError;
use domain::ports::anbu_repository::AnbuRepository;

/// Use case : lister les checkpoints ANBU d'un dépôt.
pub struct ListCheckpointsUseCase {
    anbu_repo: Arc<dyn AnbuRepository>,
}

impl ListCheckpointsUseCase {
    pub fn new(anbu_repo: Arc<dyn AnbuRepository>) -> Self {
        Self { anbu_repo }
    }

    /// Liste les checkpoints d'un dépôt.
    pub async fn execute(
        &self,
        repository_id: &Uuid,
        limit: usize,
    ) -> Result<Vec<AnbuCheckpoint>, DomainError> {
        info!(
            repository_id = %repository_id,
            limit,
            "ANBU: Listing checkpoints"
        );

        self.anbu_repo.list_checkpoints(repository_id, limit).await
    }

    /// Trouve un checkpoint par ID.
    pub async fn find(&self, id: &Uuid) -> Result<Option<AnbuCheckpoint>, DomainError> {
        self.anbu_repo.find_checkpoint(id).await
    }
}
