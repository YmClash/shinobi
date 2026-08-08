//! Use Case: UpdateIssue — Mise à jour titre/body d'une issue.
//!
//! Génère des événements de timeline pour chaque modification.

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::issue::{IssueEvent, IssueEventType};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::repo_repository::RepoRepository;

/// Commande de mise à jour d'une issue.
#[derive(Debug)]
pub struct UpdateIssueCommand {
    /// UUID de l'acteur effectuant la modification.
    pub actor_id: Uuid,
    /// UUID du dépôt.
    pub repository_id: Uuid,
    /// Numéro de l'issue.
    pub issue_number: i32,
    /// Nouveau titre (None = inchangé).
    pub title: Option<String>,
    /// Nouveau body (Some(None) = effacer, Some(Some("...")) = modifier, None = inchangé).
    pub body: Option<Option<String>>,
}

pub struct UpdateIssueUseCase {
    issue_repo: Arc<dyn IssueRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl UpdateIssueUseCase {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self { issue_repo, repo_repo }
    }

    #[instrument(skip(self), fields(actor = %cmd.actor_id, repo = %cmd.repository_id, number = cmd.issue_number))]
    pub async fn execute(&self, cmd: UpdateIssueCommand) -> Result<(), DomainError> {
        // RBAC
        let is_collab = self
            .repo_repo
            .is_collaborator(&cmd.actor_id, &cmd.repository_id)
            .await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent modifier une issue".to_string(),
            ));
        }

        let issue = self
            .issue_repo
            .find_by_repo_and_number(&cmd.repository_id, cmd.issue_number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Issue",
                id: Uuid::nil(),
            })?;

        // Mise à jour titre
        if let Some(ref new_title) = cmd.title {
            if new_title.trim().is_empty() {
                return Err(DomainError::BusinessRule(
                    "Le titre de l'issue ne peut pas être vide".to_string(),
                ));
            }
            if new_title != &issue.title {
                let old_title = issue.title.clone();
                self.issue_repo.update_title(&issue.id, new_title).await?;
                let event = IssueEvent::new(
                    issue.id,
                    cmd.actor_id,
                    IssueEventType::TitleChanged,
                    serde_json::json!({"old_title": old_title, "new_title": new_title}),
                );
                self.issue_repo.save_event(&event).await?;
            }
        }

        // Mise à jour body
        if let Some(ref new_body) = cmd.body {
            let body_str = new_body.as_deref();
            self.issue_repo.update_body(&issue.id, body_str).await?;
            let event = IssueEvent::new(
                issue.id,
                cmd.actor_id,
                IssueEventType::BodyChanged,
                serde_json::json!({}),
            );
            self.issue_repo.save_event(&event).await?;
        }

        info!(
            issue_id = %issue.id,
            number = issue.number,
            "✏️ Issue #{} mise à jour",
            issue.number
        );

        Ok(())
    }
}
