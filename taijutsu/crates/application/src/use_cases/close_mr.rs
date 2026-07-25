//! Use Case: CloseMr — Fermeture d'une MR sans fusion.

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::merge_request::{MrEvent, MrEventType, MrStatus};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;
use domain::ports::repo_repository::RepoRepository;

pub struct CloseMrUseCase {
    mr_repo: Arc<dyn MrRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl CloseMrUseCase {
    pub fn new(
        mr_repo: Arc<dyn MrRepository>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self { mr_repo, repo_repo }
    }

    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        actor_id: &Uuid,
        repository_id: &Uuid,
        mr_number: i32,
    ) -> Result<(), DomainError> {
        // RBAC — collaborateur requis
        let is_collab = self.repo_repo.is_collaborator(actor_id, repository_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent fermer une MR".to_string(),
            ));
        }

        let mr = self
            .mr_repo
            .find_by_repo_and_number(repository_id, mr_number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "MergeRequest",
                id: *repository_id,
            })?;

        if !mr.is_open() {
            return Err(DomainError::BusinessRule(
                "Impossible de fermer une MR qui n'est pas ouverte".to_string(),
            ));
        }

        let now = Utc::now();
        self.mr_repo
            .update_status(&mr.id, MrStatus::Closed, None, None, Some(now))
            .await?;

        let event = MrEvent::new(
            mr.id,
            *actor_id,
            MrEventType::Closed,
            serde_json::json!({}),
        );
        self.mr_repo.save_event(&event).await?;

        info!(
            mr_id = %mr.id,
            mr_number = mr.number,
            "✅ MR #{} fermée sans fusion",
            mr.number
        );

        Ok(())
    }
}
