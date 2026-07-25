//! Use Case: ListMrs — Liste des Merge Requests d'un dépôt.

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::entities::merge_request::{MergeRequest, MrStatus};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;

pub struct ListMrsUseCase {
    mr_repo: Arc<dyn MrRepository>,
}

impl ListMrsUseCase {
    pub fn new(mr_repo: Arc<dyn MrRepository>) -> Self {
        Self { mr_repo }
    }

    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        repo_id: &Uuid,
        status: Option<MrStatus>,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<MergeRequest>, i64), DomainError> {
        let mrs = self.mr_repo.list_by_repo(repo_id, status, limit, offset).await?;
        let total = self.mr_repo.count_by_repo(repo_id, status).await?;
        Ok((mrs, total))
    }
}
