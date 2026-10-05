//! Use Case: MrDiff — Calcul du diff d'une MR (merge-base → source).
//!
//! Utilise `VcsEngine::diff_merge_base()` pour calculer le diff correct
//! entre l'ancêtre commun et la branche source.
//!
//! ## Phase 37E — Cross-Repo MR (Le Trou de Ver Git)
//! Pour les MR cross-repo, les objets Git du fork ne sont pas dans le
//! repo parent par défaut. Ce use case déclenche un `fetch_fork_refs()`
//! automatique avant de calculer le diff — ainsi, chaque rafraîchissement
//! de la page Web met à jour le diff avec les derniers commits du fork.

use std::sync::Arc;

use tracing::{info, instrument};
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

        // ── Phase 37E — Vegapunk Tweak : lazy fetch pour cross-repo ──
        // Si la MR est cross-repo, on doit s'assurer que les refs du fork
        // sont à jour dans le parent avant de calculer le diff.
        // Le fetch local utilise des hardlinks — quasi-instantané.
        let effective_source_ref = if let Some(source_repo_id) = mr.source_repository_id {
            self.vcs
                .fetch_fork_refs(&mr.repository_id, &source_repo_id)
                .await?;

            info!(
                mr_id = %mr.id,
                source_repo = %source_repo_id,
                "🕳️ Trou de Ver — refs du fork rafraîchies pour diff"
            );

            // La ref source est accessible via le remote temporaire
            format!("fork-{}/{}", source_repo_id, mr.source_branch)
        } else {
            mr.source_branch.clone()
        };

        self.vcs
            .diff_merge_base(&mr.repository_id, &effective_source_ref, &mr.target_branch)
            .await
    }
}
