//! Use Case: GetBlob — Retourne le contenu brut d'un fichier à une révision.
//!
//! Orchestre : `ResolveRepoUseCase` → `VcsEngine::read_blob()`.

use std::sync::Arc;

use domain::errors::DomainError;
use domain::ports::vcs_engine::VcsEngine;

use crate::use_cases::resolve_repo::ResolveRepoUseCase;

/// Use Case : lire le contenu d'un fichier dans le dépôt.
pub struct GetBlobUseCase {
    vcs: Arc<dyn VcsEngine>,
    resolve_repo: Arc<ResolveRepoUseCase>,
}

impl GetBlobUseCase {
    pub fn new(
        vcs: Arc<dyn VcsEngine>,
        resolve_repo: Arc<ResolveRepoUseCase>,
    ) -> Self {
        Self { vcs, resolve_repo }
    }

    /// Retourne le contenu brut du fichier `path` à la révision `revision`.
    ///
    /// La détection du langage (pour Shiki) est faite côté handler REST.
    pub async fn execute(
        &self,
        owner: &str,
        repo: &str,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>, DomainError> {
        let repository = self.resolve_repo.execute(owner, repo).await?;
        self.vcs.read_blob(&repository.id, revision, path).await
    }
}
