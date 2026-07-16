//! Use Case: ListRefs — Liste les branches et tags d'un dépôt.
//!
//! Orchestre : `ResolveRepoUseCase` → `VcsEngine::list_refs()`.

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::vcs_engine::{RefInfo, VcsEngine};

use crate::use_cases::resolve_repo::ResolveRepoUseCase;

/// Use Case : lister les références d'un dépôt.
pub struct ListRefsUseCase {
    vcs: Arc<dyn VcsEngine>,
    resolve_repo: Arc<ResolveRepoUseCase>,
}

impl ListRefsUseCase {
    pub fn new(
        vcs: Arc<dyn VcsEngine>,
        resolve_repo: Arc<ResolveRepoUseCase>,
    ) -> Self {
        Self { vcs, resolve_repo }
    }

    pub async fn execute(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<RefInfo>, DomainError> {
        let repository = self.resolve_repo.execute(owner, repo).await?;
        self.vcs.list_refs(&repository.id).await
    }
}
