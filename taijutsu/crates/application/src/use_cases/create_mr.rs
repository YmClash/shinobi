//! Use Case: CreateMr — Ouverture d'une Merge Request.
//!
//! Orchestre la création complète d'une MR :
//! 1. Vérification RBAC (collaborateur du repo)
//! 2. Validation des branches (source ≠ target, pas de MR ouverte identique)
//! 3. Attribution atomique du numéro séquentiel
//! 4. Persistence + événement timeline
//! 5. Extraction et traitement des @mentions (Phase 37C)

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::merge_request::{MergeRequest, MrEvent, MrEventType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::mr_repository::MrRepository;
use domain::ports::repo_repository::RepoRepository;

use crate::use_cases::mention_service;

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
}

impl CreateMrUseCase {
    pub fn new(
        mr_repo: Arc<dyn MrRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
    ) -> Self {
        Self { mr_repo, repo_repo, actor_repo }
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
        let mention_text = format!(
            "{}\n{}",
            &mr.title,
            mr.description.as_deref().unwrap_or("")
        );
        let mention_result = mention_service::process_mentions(
            &mention_text,
            &cmd.author_id,
            &self.actor_repo,
        ).await;

        for resolved in &mention_result.resolved {
            let mention_event = MrEvent::new(
                mr.id,
                cmd.author_id,
                MrEventType::Mentioned,
                serde_json::json!({
                    "mentioned_actor_id": resolved.actor_id,
                    "mentioned_handle": resolved.handle,
                }),
            );
            self.mr_repo.save_event(&mention_event).await?;
        }

        info!(
            mr_id = %mr.id,
            number = mr.number,
            source = %mr.source_branch,
            target = %mr.target_branch,
            mentions = mention_result.resolved.len(),
            "✅ MR #{} créée",
            mr.number
        );

        Ok(mr)
    }
}
