//! Use Case: ReviewMr — Soumission d'une review sur une MR.

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::merge_request::{MrEvent, MrEventType, MrReview, MrVerdict};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;
use domain::ports::repo_repository::RepoRepository;

/// Commande de review d'une MR.
#[derive(Debug)]
pub struct ReviewMrCommand {
    /// UUID du reviewer.
    pub reviewer_id: Uuid,
    /// UUID du dépôt.
    pub repository_id: Uuid,
    /// Numéro de la MR.
    pub mr_number: i32,
    /// Verdict.
    pub verdict: MrVerdict,
    /// Corps de la review (Markdown).
    pub body: Option<String>,
}

pub struct ReviewMrUseCase {
    mr_repo: Arc<dyn MrRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl ReviewMrUseCase {
    pub fn new(
        mr_repo: Arc<dyn MrRepository>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self { mr_repo, repo_repo }
    }

    #[instrument(skip(self), fields(reviewer = %cmd.reviewer_id, repo = %cmd.repository_id, mr = cmd.mr_number))]
    pub async fn execute(&self, cmd: ReviewMrCommand) -> Result<MrReview, DomainError> {
        // 1. RBAC — vérifier que le reviewer est collaborateur
        let is_collab = self
            .repo_repo
            .is_collaborator(&cmd.reviewer_id, &cmd.repository_id)
            .await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent reviewer une MR".to_string(),
            ));
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
                "Impossible de reviewer une MR qui n'est pas ouverte".to_string(),
            ));
        }

        // 4. Créer la review
        let review = MrReview::new(mr.id, cmd.reviewer_id, cmd.verdict, cmd.body);
        self.mr_repo.save_review(&review).await?;

        // 5. Événement timeline
        let event_type = match cmd.verdict {
            MrVerdict::Approve => MrEventType::Approved,
            MrVerdict::ChangesRequested => MrEventType::ChangesRequested,
        };
        let event = MrEvent::new(
            mr.id,
            cmd.reviewer_id,
            event_type,
            serde_json::json!({"verdict": cmd.verdict.as_sql_str()}),
        );
        self.mr_repo.save_event(&event).await?;

        info!(
            mr_id = %mr.id,
            mr_number = mr.number,
            verdict = %cmd.verdict.as_sql_str(),
            "✅ Review soumise sur MR #{}",
            mr.number
        );

        Ok(review)
    }
}
