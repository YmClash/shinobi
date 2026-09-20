//! Use Case: CloseIssue — Fermer ou rouvrir une issue.
//!
//! Toggle : si l'issue est ouverte → fermer, si fermée → rouvrir.
//! Génère l'événement Closed ou Reopened dans la timeline.

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::issue::{IssueEvent, IssueEventType, IssueStatus};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::event_publisher::EventPublisher;

use crate::use_cases::webhook_emit;

pub struct CloseIssueUseCase {
    issue_repo: Arc<dyn IssueRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    /// Phase 34-V2 — Émission webhook (optionnel si Chakra désactivé).
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl CloseIssueUseCase {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self { issue_repo, repo_repo, event_publisher }
    }

    /// Fermer une issue ouverte.
    #[instrument(skip(self), fields(actor = %actor_id, repo = %repo_id, number))]
    pub async fn close(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<(), DomainError> {
        // RBAC
        let is_collab = self.repo_repo.is_collaborator(actor_id, repo_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent fermer une issue".to_string(),
            ));
        }

        let issue = self
            .issue_repo
            .find_by_repo_and_number(repo_id, number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Issue",
                id: Uuid::nil(),
            })?;

        if issue.is_closed() {
            return Err(DomainError::BusinessRule(
                "L'issue est déjà fermée".to_string(),
            ));
        }

        let now = Utc::now();
        self.issue_repo
            .update_status(&issue.id, IssueStatus::Closed, Some(*actor_id), Some(now))
            .await?;

        let event = IssueEvent::new(
            issue.id,
            *actor_id,
            IssueEventType::Closed,
            serde_json::json!({}),
        );
        self.issue_repo.save_event(&event).await?;

        info!(number = issue.number, "🔒 Issue #{} fermée", issue.number);

        // Phase 34-V2 — Webhook IssueClosed (fire-and-forget via Kafka)
        webhook_emit::emit_webhook_fire_and_forget(
            &self.event_publisher,
            domain::entities::webhook::WebhookEventType::IssueClosed,
            *repo_id,
            *actor_id,
            serde_json::json!({
                "action": "closed",
                "number": issue.number,
                "issue": {
                    "id": issue.id,
                    "number": issue.number,
                    "title": &issue.title,
                },
                "repository": { "id": repo_id },
                "sender": { "id": actor_id },
            }),
        );

        Ok(())
    }

    /// Rouvrir une issue fermée.
    #[instrument(skip(self), fields(actor = %actor_id, repo = %repo_id, number))]
    pub async fn reopen(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<(), DomainError> {
        // RBAC
        let is_collab = self.repo_repo.is_collaborator(actor_id, repo_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent rouvrir une issue".to_string(),
            ));
        }

        let issue = self
            .issue_repo
            .find_by_repo_and_number(repo_id, number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Issue",
                id: Uuid::nil(),
            })?;

        if issue.is_open() {
            return Err(DomainError::BusinessRule(
                "L'issue est déjà ouverte".to_string(),
            ));
        }

        self.issue_repo
            .update_status(&issue.id, IssueStatus::Open, None, None)
            .await?;

        let event = IssueEvent::new(
            issue.id,
            *actor_id,
            IssueEventType::Reopened,
            serde_json::json!({}),
        );
        self.issue_repo.save_event(&event).await?;

        info!(number = issue.number, "🔓 Issue #{} rouverte", issue.number);
        Ok(())
    }
}
