//! Use Case: ListOperations — Lister les opérations selon un filtre.

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;

/// Filtre de recherche pour les opérations.
#[derive(Debug)]
pub enum ListFilter {
    /// Les N opérations les plus récentes.
    Recent { limit: usize },
    /// Toutes les opérations d'un auteur donné.
    ByAuthor { author_id: Uuid },
}

/// Use case: lister les opérations avec filtrage.
pub struct ListOperationsUseCase {
    repository: Arc<dyn OperationRepository>,
}

impl ListOperationsUseCase {
    pub fn new(repository: Arc<dyn OperationRepository>) -> Self {
        Self { repository }
    }

    /// Exécute la recherche selon le filtre fourni.
    #[instrument(skip(self))]
    pub async fn execute(&self, filter: ListFilter) -> Result<Vec<Operation>, DomainError> {
        match filter {
            ListFilter::Recent { limit } => self.repository.list_recent(limit).await,
            ListFilter::ByAuthor { author_id } => {
                self.repository.find_by_author(&author_id).await
            }
        }
    }
}
