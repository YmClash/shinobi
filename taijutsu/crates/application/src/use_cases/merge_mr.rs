//! Use Case: MergeMr — Fusion d'une Merge Request.
//!
//! Orchestre le merge complet :
//! 1. RBAC (Maintainer ou Owner requis)
//! 2. Vérification fast-forward possible
//! 3. Exécution du merge VCS
//! 4. Transition d'état + événement timeline

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::merge_request::{MergeStrategy, MrEvent, MrEventType, MrStatus};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::vcs_engine::VcsEngine;

/// Commande de merge d'une MR.
#[derive(Debug)]
pub struct MergeMrCommand {
    /// UUID de l'acteur effectuant le merge.
    pub actor_id: Uuid,
    /// UUID du dépôt.
    pub repository_id: Uuid,
    /// Numéro de la MR.
    pub mr_number: i32,
    /// Stratégie de merge.
    pub strategy: MergeStrategy,
}

/// Résultat du merge.
#[derive(Debug)]
pub struct MergeResult {
    /// SHA du commit résultant du merge.
    pub merge_commit_id: String,
}

pub struct MergeMrUseCase {
    mr_repo: Arc<dyn MrRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    vcs: Arc<dyn VcsEngine>,
}

impl MergeMrUseCase {
    pub fn new(
        mr_repo: Arc<dyn MrRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        vcs: Arc<dyn VcsEngine>,
    ) -> Self {
        Self { mr_repo, repo_repo, vcs }
    }

    #[instrument(skip(self), fields(actor = %cmd.actor_id, repo = %cmd.repository_id, mr = cmd.mr_number))]
    pub async fn execute(&self, cmd: MergeMrCommand) -> Result<MergeResult, DomainError> {
        // 1. RBAC — Maintainer ou Owner requis
        let role = self
            .repo_repo
            .get_role(&cmd.actor_id, &cmd.repository_id)
            .await?
            .ok_or_else(|| {
                DomainError::Forbidden(
                    "Seuls les collaborateurs Maintainer ou Owner peuvent merger une MR"
                        .to_string(),
                )
            })?;

        if role != "owner" && role != "maintainer" {
            return Err(DomainError::Forbidden(format!(
                "Rôle '{role}' insuffisant — Maintainer ou Owner requis pour merger"
            )));
        }

        // 2. Trouver la MR
        let mr = self
            .mr_repo
            .find_by_repo_and_number(&cmd.repository_id, cmd.mr_number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "MergeRequest",
                id: cmd.repository_id,
            })?;

        // 3. Vérifier que la MR est ouverte
        if !mr.is_open() {
            return Err(DomainError::BusinessRule(
                "Impossible de merger une MR qui n'est pas ouverte".to_string(),
            ));
        }

        // 4. Vérifier fast-forward
        let can_ff = self
            .vcs
            .can_fast_forward(&mr.repository_id, &mr.source_branch, &mr.target_branch)
            .await?;

        if !can_ff {
            return Err(DomainError::MergeConflict {
                source_branch: mr.source_branch.clone(),
                target_branch: mr.target_branch.clone(),
            });
        }

        // 5. Exécuter le merge VCS
        let merge_cid = match cmd.strategy {
            MergeStrategy::FastForward => {
                self.vcs
                    .merge_fast_forward(&mr.repository_id, &mr.source_branch, &mr.target_branch)
                    .await?
            }
            MergeStrategy::Squash => {
                let message = format!(
                    "Merge #{}: {} (squash)\n\n{}",
                    mr.number,
                    mr.title,
                    mr.description.as_deref().unwrap_or("")
                );
                self.vcs
                    .squash_merge(
                        &mr.repository_id,
                        &mr.source_branch,
                        &mr.target_branch,
                        &message,
                    )
                    .await?
            }
        };

        // 6. Transition d'état
        let now = Utc::now();
        self.mr_repo
            .update_status(
                &mr.id,
                MrStatus::Merged,
                Some(cmd.actor_id),
                Some(now),
                None,
            )
            .await?;

        // 7. Événement timeline
        let event = MrEvent::new(
            mr.id,
            cmd.actor_id,
            MrEventType::Merged,
            serde_json::json!({
                "strategy": cmd.strategy,
                "commit_id": merge_cid.as_str(),
            }),
        );
        self.mr_repo.save_event(&event).await?;

        info!(
            mr_id = %mr.id,
            mr_number = mr.number,
            commit = %merge_cid.as_str(),
            "✅ MR #{} fusionnée avec succès",
            mr.number
        );

        Ok(MergeResult {
            merge_commit_id: merge_cid.to_string(),
        })
    }
}
