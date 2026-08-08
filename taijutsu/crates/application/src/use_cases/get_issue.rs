//! Use Case: GetIssue — Détail complet d'une issue.
//!
//! Retourne l'issue avec ses commentaires, événements et labels.

use std::sync::Arc;

use domain::entities::issue::{Issue, IssueComment, IssueEvent, IssueLabel};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;
use uuid::Uuid;

/// Résultat enrichi d'une issue.
pub struct IssueDetail {
    pub issue: Issue,
    pub comments: Vec<IssueComment>,
    pub events: Vec<IssueEvent>,
    pub labels: Vec<IssueLabel>,
}

pub struct GetIssueUseCase {
    issue_repo: Arc<dyn IssueRepository>,
}

impl GetIssueUseCase {
    pub fn new(issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { issue_repo }
    }

    pub async fn execute(
        &self,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<IssueDetail, DomainError> {
        let issue = self
            .issue_repo
            .find_by_repo_and_number(repo_id, number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Issue",
                id: Uuid::nil(), // numéro, pas UUID
            })?;

        let comments = self.issue_repo.list_comments(&issue.id).await?;
        let events = self.issue_repo.list_events(&issue.id).await?;
        let labels = self.issue_repo.get_issue_labels(&issue.id).await?;

        Ok(IssueDetail {
            issue,
            comments,
            events,
            labels,
        })
    }
}
