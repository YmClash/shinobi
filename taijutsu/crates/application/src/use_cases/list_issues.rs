//! Use Case: ListIssues — Liste paginée des issues d'un repo.

use std::sync::Arc;

use domain::entities::issue::{Issue, IssueStatus};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;

pub struct ListIssuesUseCase {
    issue_repo: Arc<dyn IssueRepository>,
}

impl ListIssuesUseCase {
    pub fn new(issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { issue_repo }
    }

    /// Retourne (issues, total_count).
    pub async fn execute(
        &self,
        repo_id: &uuid::Uuid,
        status: Option<IssueStatus>,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Issue>, i64), DomainError> {
        let issues = self
            .issue_repo
            .list_by_repo(repo_id, status, limit, offset)
            .await?;
        let total = self.issue_repo.count_by_repo(repo_id, status).await?;
        Ok((issues, total))
    }
}
