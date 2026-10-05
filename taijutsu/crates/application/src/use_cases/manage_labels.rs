//! Use Case: ManageLabels — CRUD labels + assign/unassign à une issue.

use std::sync::Arc;
use tracing::{info, instrument};
use uuid::Uuid;
use domain::entities::issue::{IssueEvent, IssueEventType, IssueLabel};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::repo_repository::RepoRepository;

pub struct ManageLabelsUseCase {
    issue_repo: Arc<dyn IssueRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl ManageLabelsUseCase {
    pub fn new(issue_repo: Arc<dyn IssueRepository>, repo_repo: Arc<dyn RepoRepository>) -> Self {
        Self { issue_repo, repo_repo }
    }

    #[instrument(skip(self), fields(repo = %repo_id))]
    pub async fn create_label(&self, actor_id: &Uuid, repo_id: &Uuid, name: String, color: String, description: Option<String>) -> Result<IssueLabel, DomainError> {
        let is_collab = self.repo_repo.is_collaborator(actor_id, repo_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden("Seuls les collaborateurs peuvent créer des labels".to_string()));
        }
        if name.trim().is_empty() {
            return Err(DomainError::BusinessRule("Le nom du label ne peut pas être vide".to_string()));
        }
        let label = IssueLabel::new(*repo_id, name, color, description);
        self.issue_repo.save_label(&label).await?;
        info!(label_id = %label.id, name = %label.name, "🏷️ Label créé");
        Ok(label)
    }

    pub async fn list_labels(&self, repo_id: &Uuid) -> Result<Vec<IssueLabel>, DomainError> {
        self.issue_repo.list_labels(repo_id).await
    }

    pub async fn delete_label(&self, actor_id: &Uuid, repo_id: &Uuid, label_id: &Uuid) -> Result<bool, DomainError> {
        let is_collab = self.repo_repo.is_collaborator(actor_id, repo_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden("Seuls les collaborateurs peuvent supprimer des labels".to_string()));
        }
        self.issue_repo.delete_label(label_id).await
    }

    pub async fn add_label(&self, actor_id: &Uuid, repo_id: &Uuid, issue_number: i32, label_id: &Uuid) -> Result<(), DomainError> {
        let is_collab = self.repo_repo.is_collaborator(actor_id, repo_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden("Seuls les collaborateurs peuvent ajouter des labels".to_string()));
        }
        let issue = self.issue_repo.find_by_repo_and_number(repo_id, issue_number).await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "Issue", id: Uuid::nil() })?;
        self.issue_repo.add_label_to_issue(&issue.id, label_id).await?;
        let event = IssueEvent::new(issue.id, *actor_id, IssueEventType::LabelAdded, serde_json::json!({"label_id": label_id}));
        self.issue_repo.save_event(&event).await?;
        Ok(())
    }

    pub async fn remove_label(&self, actor_id: &Uuid, repo_id: &Uuid, issue_number: i32, label_id: &Uuid) -> Result<(), DomainError> {
        let is_collab = self.repo_repo.is_collaborator(actor_id, repo_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden("Seuls les collaborateurs peuvent retirer des labels".to_string()));
        }
        let issue = self.issue_repo.find_by_repo_and_number(repo_id, issue_number).await?
            .ok_or_else(|| DomainError::NotFound { entity_type: "Issue", id: Uuid::nil() })?;
        self.issue_repo.remove_label_from_issue(&issue.id, label_id).await?;
        let event = IssueEvent::new(issue.id, *actor_id, IssueEventType::LabelRemoved, serde_json::json!({"label_id": label_id}));
        self.issue_repo.save_event(&event).await?;
        Ok(())
    }
}
