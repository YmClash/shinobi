//! Use Case: CreateIssue — Ouverture d'un ticket.
//!
//! Orchestre la création complète d'une issue :
//! 1. Vérification RBAC (collaborateur du repo)
//! 2. Validation métier (titre non vide)
//! 3. Attribution atomique du numéro séquentiel (compteur partagé MR/Issue)
//! 4. Persistence + événement timeline + labels optionnels
//! 5. Extraction et traitement des @mentions (Phase 37C)

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::issue::{Issue, IssueEvent, IssueEventType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::repo_repository::RepoRepository;

use crate::use_cases::mention_service;

/// Commande de création d'une issue.
#[derive(Debug)]
pub struct CreateIssueCommand {
    /// UUID de l'acteur ouvrant l'issue.
    pub author_id: Uuid,
    /// UUID du dépôt.
    pub repository_id: Uuid,
    /// Titre court.
    pub title: String,
    /// Description Markdown (optionnel).
    pub body: Option<String>,
    /// Labels à assigner à l'issue (optionnel).
    pub label_ids: Vec<Uuid>,
}

pub struct CreateIssueUseCase {
    issue_repo: Arc<dyn IssueRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    actor_repo: Arc<dyn ActorRepository>,
}

impl CreateIssueUseCase {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
    ) -> Self {
        Self { issue_repo, repo_repo, actor_repo }
    }

    #[instrument(skip(self), fields(author = %cmd.author_id, repo = %cmd.repository_id))]
    pub async fn execute(&self, cmd: CreateIssueCommand) -> Result<Issue, DomainError> {
        // 1. RBAC — sur un repo public, tout utilisateur authentifié peut ouvrir une issue.
        //    Sur un repo privé, seuls les collaborateurs peuvent le faire.
        let repo_entity = self.repo_repo.find_by_id(&cmd.repository_id).await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: cmd.repository_id,
            })?;

        if repo_entity.visibility == domain::entities::repository::Visibility::Private {
            let is_collab = self
                .repo_repo
                .is_collaborator(&cmd.author_id, &cmd.repository_id)
                .await?;
            if !is_collab {
                return Err(DomainError::Forbidden(
                    "Seuls les collaborateurs peuvent ouvrir une issue sur un dépôt privé".to_string(),
                ));
            }
        }

        // 2. Validation métier
        if cmd.title.trim().is_empty() {
            return Err(DomainError::BusinessRule(
                "Le titre de l'issue ne peut pas être vide".to_string(),
            ));
        }

        // 3. Numéro atomique (compteur partagé MR/Issue — anti race-condition)
        let number = self.issue_repo.next_number(&cmd.repository_id).await?;

        // 4. Construire et persister
        let issue = Issue::new(
            cmd.repository_id,
            cmd.author_id,
            number,
            cmd.title,
            cmd.body,
        );

        self.issue_repo.save(&issue).await?;

        // 5. Événement timeline
        let event = IssueEvent::new(
            issue.id,
            cmd.author_id,
            IssueEventType::Opened,
            serde_json::json!({"title": &issue.title}),
        );
        self.issue_repo.save_event(&event).await?;

        // 6. Labels optionnels
        for label_id in &cmd.label_ids {
            self.issue_repo.add_label_to_issue(&issue.id, label_id).await?;
            let label_event = IssueEvent::new(
                issue.id,
                cmd.author_id,
                IssueEventType::LabelAdded,
                serde_json::json!({"label_id": label_id}),
            );
            self.issue_repo.save_event(&label_event).await?;
        }

        // 7. Phase 37C — Extraction et traitement des @mentions
        let mention_text = format!(
            "{}\n{}",
            &issue.title,
            issue.body.as_deref().unwrap_or("")
        );
        let mention_result = mention_service::process_mentions(
            &mention_text,
            &cmd.author_id,
            &self.actor_repo,
        ).await;

        for resolved in &mention_result.resolved {
            let mention_event = IssueEvent::new(
                issue.id,
                cmd.author_id,
                IssueEventType::Mentioned,
                serde_json::json!({
                    "mentioned_actor_id": resolved.actor_id,
                    "mentioned_handle": resolved.handle,
                }),
            );
            self.issue_repo.save_event(&mention_event).await?;
        }

        info!(
            issue_id = %issue.id,
            number = issue.number,
            mentions = mention_result.resolved.len(),
            "✅ Issue #{} créée",
            issue.number
        );

        Ok(issue)
    }
}
