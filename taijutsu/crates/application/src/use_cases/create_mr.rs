//! Use Case: CreateMr — Ouverture d'une Merge Request.
//!
//! Orchestre la création complète d'une MR :
//! 1. Vérification RBAC (collaborateur du repo)
//! 2. Validation des branches (source ≠ target, pas de MR ouverte identique)
//! 3. Attribution atomique du numéro séquentiel
//! 4. Persistence + événement timeline
//! 5. Extraction et traitement des @mentions (Phase 37C)

use std::sync::Arc;

use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::merge_request::{MergeRequest, MrEvent, MrEventType};
use domain::entities::notification::{Notification, NotificationType, TargetType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::mr_repository::MrRepository;
use domain::ports::notification_repository::NotificationRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::event_publisher::EventPublisher;

use crate::use_cases::mention_service;
use crate::use_cases::webhook_emit;

/// Commande de création d'une MR.
#[derive(Debug)]
pub struct CreateMrCommand {
    /// UUID de l'acteur ouvrant la MR.
    pub author_id: Uuid,
    /// UUID du dépôt.
    pub repository_id: Uuid,
    /// Titre court.
    pub title: String,
    /// Description Markdown (optionnel).
    pub description: Option<String>,
    /// Branche source (ex: `feature/auth`).
    pub source_branch: String,
    /// Branche cible (ex: `main`).
    pub target_branch: String,
}

pub struct CreateMrUseCase {
    mr_repo: Arc<dyn MrRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    actor_repo: Arc<dyn ActorRepository>,
    notification_repo: Arc<dyn NotificationRepository>,
    /// Phase 34-V2 — Émission webhook (optionnel si Chakra désactivé).
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl CreateMrUseCase {
    pub fn new(
        mr_repo: Arc<dyn MrRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
        notification_repo: Arc<dyn NotificationRepository>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self { mr_repo, repo_repo, actor_repo, notification_repo, event_publisher }
    }

    #[instrument(skip(self), fields(author = %cmd.author_id, repo = %cmd.repository_id))]
    pub async fn execute(&self, cmd: CreateMrCommand) -> Result<MergeRequest, DomainError> {
        // 1. RBAC — vérifier que l'auteur est collaborateur
        let is_collab = self
            .repo_repo
            .is_collaborator(&cmd.author_id, &cmd.repository_id)
            .await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent ouvrir une MR".to_string(),
            ));
        }

        // 2. Validation métier
        if cmd.source_branch == cmd.target_branch {
            return Err(DomainError::BusinessRule(
                "La branche source et cible ne peuvent pas être identiques".to_string(),
            ));
        }

        if cmd.title.trim().is_empty() {
            return Err(DomainError::BusinessRule(
                "Le titre de la MR ne peut pas être vide".to_string(),
            ));
        }

        // 3. Pas de MR ouverte avec les mêmes branches
        if let Some(existing) = self
            .mr_repo
            .find_open_by_branches(
                &cmd.repository_id,
                &cmd.source_branch,
                &cmd.target_branch,
            )
            .await?
        {
            return Err(DomainError::Duplicate(format!(
                "MR #{} déjà ouverte pour {} → {}",
                existing.number, cmd.source_branch, cmd.target_branch
            )));
        }

        // 4. Numéro atomique (anti race-condition)
        let number = self.mr_repo.next_number(&cmd.repository_id).await?;

        // 5. Construire et persister
        let mr = MergeRequest::new(
            cmd.repository_id,
            cmd.author_id,
            number,
            cmd.title,
            cmd.description,
            cmd.source_branch,
            cmd.target_branch,
        );

        self.mr_repo.save(&mr).await?;

        // 6. Événement timeline
        let event = MrEvent::new(
            mr.id,
            cmd.author_id,
            MrEventType::Opened,
            serde_json::json!({"title": &mr.title}),
        );
        self.mr_repo.save_event(&event).await?;

        // 7. Phase 37C — Extraction et traitement des @mentions
        //    P2 fix: Fire-and-forget via tokio::spawn — l'HTTP 201 revient immédiatement.
        let mention_text = format!(
            "{}\n{}",
            &mr.title,
            mr.description.as_deref().unwrap_or("")
        );
        let mr_id = mr.id;
        let mr_number = mr.number;
        let author_id = cmd.author_id;
        let actor_repo = Arc::clone(&self.actor_repo);
        let mr_repo = Arc::clone(&self.mr_repo);
        let notification_repo = Arc::clone(&self.notification_repo);
        let repo_id = cmd.repository_id;

        // Need repo entity for denormalized notification fields
        let (repo_name, repo_owner_id) = match self.repo_repo.find_by_id(&repo_id).await {
            Ok(Some(repo_entity)) => (repo_entity.name, repo_entity.owner_id),
            _ => (String::new(), Uuid::nil()),
        };

        tokio::spawn(async move {
            // Resolve owner handle for denormalized notification storage
            let owner_handle = if repo_owner_id != Uuid::nil() {
                match actor_repo.find_by_id(&repo_owner_id).await {
                    Ok(Some(actor)) => actor.handle,
                    _ => "unknown".to_string(),
                }
            } else {
                "unknown".to_string()
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
                let mention_event = MrEvent::new(
                    mr_id,
                    author_id,
                    MrEventType::Mentioned,
                    serde_json::json!({
                        "mentioned_actor_id": resolved.actor_id,
                        "mentioned_handle": resolved.handle,
                    }),
                );
                if let Err(e) = mr_repo.save_event(&mention_event).await {
                    warn!(
                        mr_id = %mr_id,
                        error = %e,
                        "⚠️ Erreur lors de la persistence d'un événement Mentioned (MR)"
                    );
                }

                // Phase 38 — Notification 🔔
                let notif = Notification::new(
                    resolved.actor_id,
                    author_id,
                    NotificationType::Mentioned,
                    TargetType::MergeRequest,
                    mr_id,
                    Some(mr_number),
                    repo_id,
                    owner_handle.clone(),
                    repo_name.clone(),
                    format!("{} mentioned you in MR #{}", author_handle, mr_number),
                );
                if let Err(e) = notification_repo.save(&notif).await {
                    warn!("⚠️ Notification error: {}", e);
                }
            }

            if !mention_result.resolved.is_empty() {
                info!(
                    mr_id = %mr_id,
                    number = mr_number,
                    mentions = mention_result.resolved.len(),
                    "📣🔔 Mentions + notifications traitées pour MR #{}",
                    mr_number
                );
            }
        });

        // Phase 34-V2 — Webhook MrCreated (fire-and-forget via Kafka)
        webhook_emit::emit_webhook_fire_and_forget(
            &self.event_publisher,
            domain::entities::webhook::WebhookEventType::MrCreated,
            cmd.repository_id,
            cmd.author_id,
            serde_json::json!({
                "action": "opened",
                "number": mr.number,
                "merge_request": {
                    "id": mr.id,
                    "number": mr.number,
                    "title": &mr.title,
                    "source_branch": &mr.source_branch,
                    "target_branch": &mr.target_branch,
                },
                "repository": { "id": cmd.repository_id },
                "sender": { "id": cmd.author_id },
            }),
        );

        info!(
            mr_id = %mr.id,
            number = mr.number,
            source = %mr.source_branch,
            target = %mr.target_branch,
            "✅ MR #{} créée",
            mr.number
        );

        Ok(mr)
    }
}
