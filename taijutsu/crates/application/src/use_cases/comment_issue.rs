//! Use Case: CommentIssue — Ajouter un commentaire à une issue.

use std::sync::Arc;
use tracing::{info, instrument};
use uuid::Uuid;
use domain::entities::issue::{IssueComment, IssueEvent, IssueEventType};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::repo_repository::RepoRepository;

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
}

impl CommentIssueUseCase {
    pub fn new(issue_repo: Arc<dyn IssueRepository>, repo_repo: Arc<dyn RepoRepository>) -> Self {
        Self { issue_repo, repo_repo }
    }

    #[instrument(skip(self), fields(author = %cmd.author_id, number = cmd.issue_number))]
    pub async fn execute(&self, cmd: CommentIssueCommand) -> Result<IssueComment, DomainError> {
        let is_collab = self.repo_repo.is_collaborator(&cmd.author_id, &cmd.repository_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden("Seuls les collaborateurs peuvent commenter".to_string()));
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

        info!(comment_id = %comment.id, "💬 Commentaire ajouté à l'issue #{}", issue.number);
        Ok(comment)
    }
}
