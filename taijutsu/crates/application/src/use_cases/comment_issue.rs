//! Use Case: CommentIssue — Ajouter un commentaire à une issue.
//!
//! Phase 37C: intègre l'extraction des @mentions dans le corps du commentaire.

use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;
use domain::entities::issue::{IssueComment, IssueEvent, IssueEventType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::repo_repository::RepoRepository;

use crate::use_cases::mention_service;

#[derive(Debug)]
pub struct CommentIssueCommand {
    pub author_id: Uuid,
    pub repository_id: Uuid,
    pub issue_number: i32,
    pub body: String,
}

pub struct CommentIssueUseCase {
    issue_repo: Arc<dyn IssueRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    actor_repo: Arc<dyn ActorRepository>,
}

impl CommentIssueUseCase {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
    ) -> Self {
        Self { issue_repo, repo_repo, actor_repo }
    }

    #[instrument(skip(self), fields(author = %cmd.author_id, number = cmd.issue_number))]
    pub async fn execute(&self, cmd: CommentIssueCommand) -> Result<IssueComment, DomainError> {
        // RBAC — sur un repo public, tout utilisateur authentifié peut commenter.
        let repo_entity = self.repo_repo.find_by_id(&cmd.repository_id).await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: cmd.repository_id,
            })?;
        if repo_entity.visibility == domain::entities::repository::Visibility::Private {
            let is_collab = self.repo_repo.is_collaborator(&cmd.author_id, &cmd.repository_id).await?;
            if !is_collab {
                return Err(DomainError::Forbidden("Seuls les collaborateurs peuvent commenter sur un dépôt privé".to_string()));
            }
        }
        if cmd.body.trim().is_empty() {
            return Err(DomainError::BusinessRule("Le commentaire ne peut pas être vide".to_string()));
        }
        let issue = self.issue_repo.find_by_repo_and_number(&cmd.repository_id, cmd.issue_number).await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "Issue", id: Uuid::nil() })?;

        let comment = IssueComment::new(issue.id, cmd.author_id, cmd.body);
        self.issue_repo.save_comment(&comment).await?;

        let event = IssueEvent::new(issue.id, cmd.author_id, IssueEventType::Commented, serde_json::json!({"comment_id": comment.id}));
        self.issue_repo.save_event(&event).await?;

        // Phase 37C — Extraction des @mentions dans le commentaire
        //    P2 fix: Fire-and-forget via tokio::spawn — l'HTTP 201 revient immédiatement.
        let comment_body = comment.body.clone();
        let comment_id = comment.id;
        let issue_id = issue.id;
        let issue_number = issue.number;
        let author_id = cmd.author_id;
        let actor_repo = Arc::clone(&self.actor_repo);
        let issue_repo = Arc::clone(&self.issue_repo);

        tokio::spawn(async move {
            let mention_result = mention_service::process_mentions(
                &comment_body,
                &author_id,
                &actor_repo,
            ).await;

            for resolved in &mention_result.resolved {
                let mention_event = IssueEvent::new(
                    issue_id,
                    author_id,
                    IssueEventType::Mentioned,
                    serde_json::json!({
                        "mentioned_actor_id": resolved.actor_id,
                        "mentioned_handle": resolved.handle,
                        "comment_id": comment_id,
                    }),
                );
                if let Err(e) = issue_repo.save_event(&mention_event).await {
                    warn!(
                        issue_id = %issue_id,
                        error = %e,
                        "⚠️ Erreur lors de la persistence d'un événement Mentioned (commentaire)"
                    );
                }
            }

            if !mention_result.resolved.is_empty() {
                info!(
                    comment_id = %comment_id,
                    mentions = mention_result.resolved.len(),
                    "📣 Mentions traitées (fire-and-forget) pour commentaire sur Issue #{}",
                    issue_number
                );
            }
        });

        info!(
            comment_id = %comment.id,
            "💬 Commentaire ajouté à l'issue #{}",
            issue.number,
        );
        Ok(comment)
    }
}
