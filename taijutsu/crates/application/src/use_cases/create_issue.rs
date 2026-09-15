//! Use Case: CreateIssue — Ouverture d'un ticket.
//!
//! Orchestre la création complète d'une issue :
//! 1. Vérification RBAC (collaborateur du repo)
//! 2. Validation métier (titre non vide)
//! 3. Attribution atomique du numéro séquentiel (compteur partagé MR/Issue)
//! 4. Persistence + événement timeline + labels optionnels
//! 5. Extraction et traitement des @mentions (Phase 37C)
//! 6. Phase 37F — Route les mentions fédérées via ActivityPub

use std::sync::Arc;

use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::issue::{Issue, IssueEvent, IssueEventType};
use domain::entities::notification::{Notification, NotificationType, TargetType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::federation_repository::FederationRepository;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::notification_repository::NotificationRepository;
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
    notification_repo: Arc<dyn NotificationRepository>,
    // Phase 37F — Mégaphone Interstellaire
    federation_repo: Arc<dyn FederationRepository>,
    remote_fetcher: Arc<infrastructure::federation::remote_actor::RemoteActorFetcher>,
    federation_domain: String,
}

impl CreateIssueUseCase {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
        notification_repo: Arc<dyn NotificationRepository>,
        federation_repo: Arc<dyn FederationRepository>,
        remote_fetcher: Arc<infrastructure::federation::remote_actor::RemoteActorFetcher>,
        federation_domain: String,
    ) -> Self {
        Self {
            issue_repo, repo_repo, actor_repo, notification_repo,
            federation_repo, remote_fetcher, federation_domain,
        }
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
        //    P2 fix: Fire-and-forget via tokio::spawn — l'HTTP 201 revient immédiatement.
        //    Les événements Mentioned apparaissent quelques ms plus tard.
        let mention_text = format!(
            "{}\n{}",
            &issue.title,
            issue.body.as_deref().unwrap_or("")
        );
        let issue_id = issue.id;
        let author_id = cmd.author_id;
        let actor_repo = Arc::clone(&self.actor_repo);
        let issue_repo = Arc::clone(&self.issue_repo);
        let notification_repo = Arc::clone(&self.notification_repo);
        let issue_number = issue.number;
        let repo_id = repo_entity.id;
        let repo_name = repo_entity.name.clone();
        let repo_owner_id = repo_entity.owner_id;
        // Phase 37F — capture des dépendances fédération
        let federation_repo = Arc::clone(&self.federation_repo);
        let remote_fetcher = Arc::clone(&self.remote_fetcher);
        let federation_domain = self.federation_domain.clone();

        tokio::spawn(async move {
            // Resolve owner handle for denormalized notification storage
            let owner_handle = match actor_repo.find_by_id(&repo_owner_id).await {
                Ok(Some(actor)) => actor.handle,
                _ => "unknown".to_string(),
            };

            let mention_result = mention_service::process_mentions(
                &mention_text,
                &author_id,
                &actor_repo,
            ).await;

            // Resolve author handle for notification message
            let author_handle = match actor_repo.find_by_id(&author_id).await {
                Ok(Some(actor)) => actor.handle,
                _ => "someone".to_string(),
            };

            for resolved in &mention_result.resolved {
                let mention_event = IssueEvent::new(
                    issue_id,
                    author_id,
                    IssueEventType::Mentioned,
                    serde_json::json!({
                        "mentioned_actor_id": resolved.actor_id,
                        "mentioned_handle": resolved.handle,
                    }),
                );
                if let Err(e) = issue_repo.save_event(&mention_event).await {
                    warn!(
                        issue_id = %issue_id,
                        error = %e,
                        "⚠️ Erreur lors de la persistence d'un événement Mentioned"
                    );
                }

                // Phase 38 — Notification 🔔
                let notif = Notification::new(
                    resolved.actor_id,
                    author_id,
                    NotificationType::Mentioned,
                    TargetType::Issue,
                    issue_id,
                    Some(issue_number),
                    repo_id,
                    owner_handle.clone(),
                    repo_name.clone(),
                    format!("{} mentioned you in Issue #{}", author_handle, issue_number),
                );
                if let Err(e) = notification_repo.save(&notif).await {
                    warn!("⚠️ Notification error: {}", e);
                }
            }

            if !mention_result.resolved.is_empty() {
                info!(
                    issue_id = %issue_id,
                    number = issue_number,
                    mentions = mention_result.resolved.len(),
                    "📣🔔 Mentions + notifications traitées pour Issue #{}",
                    issue_number
                );
            }

            // ── Phase 37F — Mégaphone Interstellaire 📡 ──
            // Route les mentions distantes (@handle@domain) via ActivityPub
            if !mention_result.remote.is_empty() {
                let context_url = format!(
                    "https://{}/{}/{}/issues/{}",
                    federation_domain.replace("api.", ""),
                    owner_handle,
                    repo_name,
                    issue_number,
                );
                mention_service::deliver_remote_mentions(
                    mention_result.remote,
                    &author_id,
                    &context_url,
                    &mention_text,
                    &issue_id.to_string(),
                    &federation_domain,
                    &actor_repo,
                    &federation_repo,
                    &remote_fetcher,
                ).await;
            }
        });

        info!(
            issue_id = %issue.id,
            number = issue.number,
            "✅ Issue #{} créée",
            issue.number
        );

        Ok(issue)
    }
}
