//! Use Case: CloseMr — Fermeture d'une MR sans fusion.
//!
//! ## Phase 37E — Cross-Repo MR
//! Si la MR est cross-repo, le cleanup du remote temporaire est déclenché
//! à la fermeture (même logique finally-style que le merge).

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::merge_request::{MrEvent, MrEventType, MrStatus};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::vcs_engine::VcsEngine;
use domain::ports::event_publisher::EventPublisher;

use crate::use_cases::webhook_emit;

pub struct CloseMrUseCase {
    mr_repo: Arc<dyn MrRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    vcs: Arc<dyn VcsEngine>,
    /// Phase 34-V2 — Émission webhook (optionnel si Chakra désactivé).
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl CloseMrUseCase {
    pub fn new(
        mr_repo: Arc<dyn MrRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        vcs: Arc<dyn VcsEngine>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self { mr_repo, repo_repo, vcs, event_publisher }
    }

    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        actor_id: &Uuid,
        repository_id: &Uuid,
        mr_number: i32,
    ) -> Result<(), DomainError> {
        // RBAC — collaborateur requis
        let is_collab = self.repo_repo.is_collaborator(actor_id, repository_id).await?;
        if !is_collab {
            return Err(DomainError::Forbidden(
                "Seuls les collaborateurs peuvent fermer une MR".to_string(),
            ));
        }

        let mr = self
            .mr_repo
            .find_by_repo_and_number(repository_id, mr_number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "MergeRequest",
                id: *repository_id,
            })?;

        if !mr.is_open() {
            return Err(DomainError::BusinessRule(
                "Impossible de fermer une MR qui n'est pas ouverte".to_string(),
            ));
        }

        let now = Utc::now();
        self.mr_repo
            .update_status(&mr.id, MrStatus::Closed, None, None, Some(now))
            .await?;

        let event = MrEvent::new(
            mr.id,
            *actor_id,
            MrEventType::Closed,
            serde_json::json!({}),
        );
        self.mr_repo.save_event(&event).await?;

        // ── Phase 37E — Cleanup du Trou de Ver sur Close ────────────
        if let Some(source_repo_id) = mr.source_repository_id {
            if let Err(e) = self.vcs.cleanup_fork_remote(&mr.repository_id, &source_repo_id).await {
                warn!(
                    mr_id = %mr.id,
                    source_repo = %source_repo_id,
                    error = %e,
                    "⚠️ Cleanup fork remote échoué à la fermeture (non-fatal)"
                );
            } else {
                info!(
                    mr_id = %mr.id,
                    source_repo = %source_repo_id,
                    "🧹 Remote fork nettoyé suite à la fermeture de la MR cross-repo"
                );
            }
        }

        info!(
            mr_id = %mr.id,
            mr_number = mr.number,
            cross_repo = mr.is_cross_repo(),
            "✅ MR #{} fermée sans fusion{}",
            mr.number,
            if mr.is_cross_repo() { " (cross-repo 🕳️)" } else { "" }
        );

        // Phase 34-V2 — Webhook MrClosed (fire-and-forget via Kafka)
        webhook_emit::emit_webhook_fire_and_forget(
            &self.event_publisher,
            domain::entities::webhook::WebhookEventType::MrClosed,
            *repository_id,
            *actor_id,
            serde_json::json!({
                "action": "closed",
                "number": mr.number,
                "merge_request": {
                    "id": mr.id,
                    "number": mr.number,
                    "title": &mr.title,
                    "source_branch": &mr.source_branch,
                    "target_branch": &mr.target_branch,
                    "cross_repo": mr.is_cross_repo(),
                },
                "repository": { "id": repository_id },
                "sender": { "id": actor_id },
            }),
        );

        Ok(())
    }
}
