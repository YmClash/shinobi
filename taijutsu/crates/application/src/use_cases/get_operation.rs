//! Use Case: GetOperation — Retrouver une opération par son identifiant.

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;

/// Use case: retrouver une opération par son ID.
pub struct GetOperationUseCase {
    repository: Arc<dyn OperationRepository>,
}

impl GetOperationUseCase {
    pub fn new(repository: Arc<dyn OperationRepository>) -> Self {
        Self { repository }
    }

    /// Exécute la recherche. Retourne `NotFound` si l'opération n'existe pas.
    #[instrument(skip(self))]
    pub async fn execute(&self, id: Uuid) -> Result<Operation, DomainError> {
        self.repository
            .find_by_id(&id)
            .await?
            .ok_or(DomainError::NotFound {
                entity_type: "Operation",
                id,
            })
    }
}
