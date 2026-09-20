//! Use Case: CommentIssue — Ajouter un commentaire à une issue.
//!
//! Phase 37C: intègre l'extraction des @mentions dans le corps du commentaire.
//! Phase 37F: route les mentions fédérées via ActivityPub (Le Mégaphone Interstellaire).

use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;
use domain::entities::issue::{IssueComment, IssueEvent, IssueEventType};
use domain::entities::notification::{Notification, NotificationType, TargetType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::federation_repository::FederationRepository;
use domain::ports::issue_repository::IssueRepository;
use domain::ports::notification_repository::NotificationRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::event_publisher::EventPublisher;

use crate::use_cases::mention_service;
use crate::use_cases::webhook_emit;

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
    notification_repo: Arc<dyn NotificationRepository>,
    // Phase 37F — Mégaphone Interstellaire
    federation_repo: Arc<dyn FederationRepository>,
    remote_fetcher: Arc<infrastructure::federation::remote_actor::RemoteActorFetcher>,
    federation_domain: String,
    /// Phase 34-V2 — Émission webhook (optionnel si Chakra désactivé).
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl CommentIssueUseCase {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
        notification_repo: Arc<dyn NotificationRepository>,
        federation_repo: Arc<dyn FederationRepository>,
        remote_fetcher: Arc<infrastructure::federation::remote_actor::RemoteActorFetcher>,
        federation_domain: String,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self {
            issue_repo, repo_repo, actor_repo, notification_repo,
            federation_repo, remote_fetcher, federation_domain,
            event_publisher,
        }
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
        let notification_repo = Arc::clone(&self.notification_repo);
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
                &comment_body,
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
                    format!("{} mentioned you in a comment on Issue #{}", author_handle, issue_number),
                );
                if let Err(e) = notification_repo.save(&notif).await {
                    warn!("⚠️ Notification error: {}", e);
                }
            }

            if !mention_result.resolved.is_empty() {
                info!(
                    comment_id = %comment_id,
                    mentions = mention_result.resolved.len(),
                    "📣🔔 Mentions + notifications traitées pour commentaire sur Issue #{}",
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
                    &comment_body,
                    &comment_id.to_string(),
                    &federation_domain,
                    &actor_repo,
                    &federation_repo,
                    &remote_fetcher,
                ).await;
            }
        });

        // Phase 34-V2 — Webhook IssueComment (fire-and-forget via Kafka)
        webhook_emit::emit_webhook_fire_and_forget(
            &self.event_publisher,
            domain::entities::webhook::WebhookEventType::IssueComment,
            cmd.repository_id,
            cmd.author_id,
            serde_json::json!({
                "action": "created",
                "comment": {
                    "id": comment.id,
                    "body": &comment.body,
                },
                "issue": { "number": cmd.issue_number },
                "repository": { "id": cmd.repository_id },
                "sender": { "id": cmd.author_id },
            }),
        );

        info!(
            comment_id = %comment.id,
            "💬 Commentaire ajouté à l'issue #{}",
            issue.number,
        );
        Ok(comment)
    }
}
