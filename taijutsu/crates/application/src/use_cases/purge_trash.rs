//! Use Case : Purge automatique des dépôts expirés (Phase 24).
//!
//! Tâche de fond qui supprime définitivement les dépôts dont le
//! `deleted_at` dépasse le délai de rétention. Exécutée périodiquement
//! par un timer `tokio::spawn + interval` dans main.rs.
//!
//! ## Nettoyage
//! 1. Hard delete en DB (CASCADE : operations, chunks, reviews, collaborators)
//! 2. Suppression du workspace VCS sur le disque

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, instrument, warn};

use domain::errors::DomainError;
use domain::ports::repo_repository::RepoRepository;

/// Durée de rétention en secondes avant purge définitive.
/// 1 heure pour les tests, 604800 (7 jours) pour la production.
pub const TRASH_RETENTION_SECS: i64 = 3600; // 1h (test) — TODO: 604800 (7j) en prod

/// Purge automatique des dépôts expirés dans la corbeille.
pub struct PurgeTrashUseCase {
    repo_repo: Arc<dyn RepoRepository>,
    workspace_root: PathBuf,
}

impl PurgeTrashUseCase {
    pub fn new(
        repo_repo: Arc<dyn RepoRepository>,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            repo_repo,
            workspace_root,
        }
    }

    /// Purge tous les repos dont `deleted_at` dépasse le délai de rétention.
    ///
    /// Pour chaque repo expiré :
    /// 1. Hard delete en DB (CASCADE supprime operations, chunks, reviews, collaborators)
    /// 2. Suppression du dossier VCS sur le disque
    ///
    /// Retourne le nombre de dépôts purgés.
    #[instrument(skip(self))]
    pub async fn execute(&self) -> Result<u64, DomainError> {
        let expired = self
            .repo_repo
            .list_expired_trash(TRASH_RETENTION_SECS)
            .await?;

        if expired.is_empty() {
            return Ok(0);
        }

        info!(
            count = expired.len(),
            "🗑️ Purge: {} dépôt(s) expiré(s) à supprimer définitivement",
            expired.len()
        );

        let mut purged = 0u64;

        for repo in &expired {
            // 1. Hard delete en DB (CASCADE)
            match self.repo_repo.hard_delete(&repo.id).await {
                Ok(true) => {
                    info!(
                        repo_id = %repo.id,
                        repo_name = %repo.name,
                        "🔥 Dépôt purgé définitivement de la DB"
                    );
                }
                Ok(false) => {
                    warn!(repo_id = %repo.id, "Purge: repo déjà supprimé de la DB");
                    continue;
                }
                Err(e) => {
                    warn!(repo_id = %repo.id, error = %e, "Purge: erreur lors du hard delete");
                    continue;
                }
            }

            // 2. Supprimer le workspace VCS du disque
            let vcs_path = self
                .workspace_root
                .join(repo.owner_id.to_string())
                .join(repo.id.to_string());

            if vcs_path.exists() {
                match tokio::fs::remove_dir_all(&vcs_path).await {
                    Ok(()) => {
                        info!(
                            path = %vcs_path.display(),
                            "🧹 Workspace VCS supprimé du disque"
                        );
                    }
                    Err(e) => {
                        warn!(
                            path = %vcs_path.display(),
                            error = %e,
                            "⚠️ Erreur lors de la suppression du workspace VCS"
                        );
                    }
                }
            }

            purged += 1;
        }

        Ok(purged)
    }
}
