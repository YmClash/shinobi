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
    /// Les N opérations les plus récentes d'un dépôt (Phase 10B — multi-tenant).
    Recent { repo_id: Uuid, limit: usize },
    /// Toutes les opérations d'un auteur donné.
    ByAuthor { author_id: Uuid },
}

/// Résultat paginé avec total_count absolu (Phase 17).
pub struct OperationsResult {
    /// Opérations retournées (paginées selon le filtre).
    pub operations: Vec<Operation>,
    /// Nombre total d'opérations dans le dépôt (SELECT COUNT(*)).
    /// `None` pour les filtres non liés à un dépôt (ByAuthor).
    pub total_count: Option<i64>,
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
            ListFilter::Recent { repo_id, limit } => self.repository.list_recent(&repo_id, limit).await,
            ListFilter::ByAuthor { author_id } => {
                self.repository.find_by_author(&author_id).await
            }
        }
    }

    /// Exécute la recherche avec le `total_count` absolu (Phase 17).
    ///
    /// Pour le filtre `Recent`, le total_count est calculé via un COUNT(*)
    /// indépendant du LIMIT — O(1) sur l'index PostgreSQL.
    #[instrument(skip(self))]
    pub async fn execute_with_count(&self, filter: ListFilter) -> Result<OperationsResult, DomainError> {
        match filter {
            ListFilter::Recent { repo_id, limit } => {
                let (operations, total_count) = tokio::join!(
                    self.repository.list_recent(&repo_id, limit),
                    self.repository.count_by_repo(&repo_id),
                );
                Ok(OperationsResult {
                    operations: operations?,
                    total_count: Some(total_count?),
                })
            }
            ListFilter::ByAuthor { author_id } => {
                let operations = self.repository.find_by_author(&author_id).await?;
                Ok(OperationsResult {
                    operations,
                    total_count: None,
                })
            }
        }
    }
}
