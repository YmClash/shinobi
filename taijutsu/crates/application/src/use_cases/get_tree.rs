//! Use Case: GetTree — Liste l'arborescence d'un dépôt à une révision.
//!
//! Orchestre : `ResolveRepoUseCase` → `VcsEngine::list_tree()`.
//! Retourne les entrées groupées par niveau de répertoire.

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::vcs_engine::{TreeEntry, VcsEngine};

use crate::use_cases::resolve_repo::ResolveRepoUseCase;

/// Résultat de `GetTreeUseCase`.
pub struct GetTreeResult {
    pub revision: String,
    pub path: String,
    pub entries: Vec<TreeEntry>,
}

/// Use Case : lister l'arborescence d'un dépôt.
pub struct GetTreeUseCase {
    vcs: Arc<dyn VcsEngine>,
    resolve_repo: Arc<ResolveRepoUseCase>,
}

impl GetTreeUseCase {
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
        revision: &str,
        path: &str,
    ) -> Result<GetTreeResult, DomainError> {
        let repository = self.resolve_repo.execute(owner, repo).await?;

        // Initialiser le workspace si nécessaire (Phase 21: owner_id/repo_id)
        self.vcs.init_workspace(&repository.owner_id, &repository.id).await.ok();

        let entries = self.vcs
            .list_tree(&repository.id, revision, path)
            .await?;

        Ok(GetTreeResult {
            revision: revision.to_string(),
            path: path.to_string(),
            entries,
        })
    }
}
