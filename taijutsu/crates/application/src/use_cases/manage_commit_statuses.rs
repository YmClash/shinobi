//! Use Case : Gestion des Commit Statuses — Phase 39 (Le Pont CI/CD) 🌉
//!
//! Permet aux pipelines CI/CD externes de reporter le statut de leurs
//! builds dans Shinobi via l'API REST.
//!
//! ## Méthodes
//! - `create_or_update_status()` : UPSERT un statut (POST)
//! - `list_statuses()` : Liste les statuts d'un commit (GET)
//! - `combined_status()` : Calcule le statut combiné (GET /combined)
//!
//! ## Sécurité
//! - L'acteur doit être authentifié (JWT ou PAT)
//! - Le dépôt doit exister (résolu via owner/repo_name)
//! - Validation du state au niveau applicatif (pas de VARCHAR invalide)

use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use domain::entities::commit_status::{CombinedStatus, CommitStatus, CommitStatusState};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::commit_status_repository::CommitStatusRepository;
use domain::ports::repo_repository::RepoRepository;

/// Use case CRUD pour les statuts de commit CI/CD.
pub struct ManageCommitStatusesUseCase {
    commit_status_repo: Arc<dyn CommitStatusRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    actor_repo: Arc<dyn ActorRepository>,
}

impl ManageCommitStatusesUseCase {
    /// Construit le use case.
    pub fn new(
        commit_status_repo: Arc<dyn CommitStatusRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
    ) -> Self {
        Self {
            commit_status_repo,
            repo_repo,
            actor_repo,
        }
    }

    /// Crée ou met à jour un statut de commit.
    ///
    /// # Validations
    /// - Le dépôt existe (résolu via owner/repo_name)
    /// - Le state est valide (pending | success | failure | error)
    /// - Le context n'est pas vide
    /// - Le commit_id n'est pas vide
    pub async fn create_or_update_status(
        &self,
        actor_id: Uuid,
        owner: &str,
        repo_name: &str,
        commit_id: &str,
        state: &str,
        context: &str,
        description: Option<String>,
        target_url: Option<String>,
    ) -> Result<CommitStatus, DomainError> {
        // ── 1. Valider le state ──
        let status_state = CommitStatusState::from_sql(state)
            .ok_or_else(|| DomainError::BusinessRule(
                format!("État invalide: '{}'. Valeurs autorisées: pending, success, failure, error", state),
            ))?;

        // ── 2. Valider le context ──
        if context.is_empty() {
            return Err(DomainError::BusinessRule(
                "Le context ne peut pas être vide".to_string(),
            ));
        }
        if context.len() > 255 {
            return Err(DomainError::BusinessRule(
                "Le context ne peut pas dépasser 255 caractères".to_string(),
            ));
        }

        // ── 3. Valider le commit_id ──
        if commit_id.is_empty() {
            return Err(DomainError::BusinessRule(
                "Le commit_id ne peut pas être vide".to_string(),
            ));
        }
        if commit_id.len() > 64 {
            return Err(DomainError::BusinessRule(
                "Le commit_id ne peut pas dépasser 64 caractères".to_string(),
            ));
        }

        // ── 4. Résoudre le dépôt via owner/name ──
        let owner_actor = self
            .actor_repo
            .find_by_handle(owner)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        let repo = self
            .repo_repo
            .find_by_owner_and_name(&owner_actor.id, repo_name)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: Uuid::nil(),
            })?;

        // ── 5. Construire et upsert ──
        let status = CommitStatus::new(
            repo.id,
            commit_id.to_string(),
            context.to_string(),
            status_state,
            description,
            target_url,
            Some(actor_id),
        );

        let result = self.commit_status_repo.upsert(&status).await?;

        info!(
            repo_id = %repo.id,
            commit_id = %commit_id,
            context = %context,
            state = %state,
            "🌉 Commit status upserted"
        );

        Ok(result)
    }

    /// Liste tous les statuts d'un commit.
    pub async fn list_statuses(
        &self,
        owner: &str,
        repo_name: &str,
        commit_id: &str,
    ) -> Result<Vec<CommitStatus>, DomainError> {
        let repo = self.resolve_repo(owner, repo_name).await?;
        self.commit_status_repo.list_by_commit(&repo.id, commit_id).await
    }

    /// Calcule le statut combiné d'un commit.
    ///
    /// Logique GitHub-style :
    /// - `Success` si **tous** les statuts sont `Success`
    /// - `Pending` si ≥1 est `Pending` (et aucun `Failure`/`Error`)
    /// - `Failure` sinon
    pub async fn combined_status(
        &self,
        owner: &str,
        repo_name: &str,
        commit_id: &str,
    ) -> Result<CombinedStatus, DomainError> {
        let repo = self.resolve_repo(owner, repo_name).await?;
        let statuses = self.commit_status_repo.list_by_commit(&repo.id, commit_id).await?;
        Ok(CombinedStatus::from_statuses(statuses))
    }

    // ── Helper privé ─────────────────────────────────────────────

    /// Résout un dépôt via owner/name.
    async fn resolve_repo(
        &self,
        owner: &str,
        repo_name: &str,
    ) -> Result<domain::entities::repository::Repository, DomainError> {
        let owner_actor = self
            .actor_repo
            .find_by_handle(owner)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        self.repo_repo
            .find_by_owner_and_name(&owner_actor.id, repo_name)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: Uuid::nil(),
            })
    }
}
