//! Use Case: MrDiff — Calcul du diff d'une MR (merge-base → source).
//!
//! Utilise `VcsEngine::diff_merge_base()` pour calculer le diff correct
//! entre l'ancêtre commun et la branche source.

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;
use domain::ports::vcs_engine::{FileDiff, VcsEngine};

pub struct MrDiffUseCase {
    mr_repo: Arc<dyn MrRepository>,
    vcs: Arc<dyn VcsEngine>,
}

impl MrDiffUseCase {
    pub fn new(mr_repo: Arc<dyn MrRepository>, vcs: Arc<dyn VcsEngine>) -> Self {
        Self { mr_repo, vcs }
    }

    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        repo_id: &Uuid,
        mr_number: i32,
    ) -> Result<Vec<FileDiff>, DomainError> {
        let mr = self
            .mr_repo
            .find_by_repo_and_number(repo_id, mr_number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "MergeRequest",
                id: *repo_id,
            })?;

        self.vcs
            .diff_merge_base(&mr.repository_id, &mr.source_branch, &mr.target_branch)
            .await
    }
}
