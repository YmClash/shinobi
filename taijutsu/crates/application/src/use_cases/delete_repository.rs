//! Use Case : Supprimer / Restaurer un dépôt (Phase 24 — Le Sceau Brisé).
//!
//! Soft delete : le repo est marqué `deleted_at = NOW()` et disparaît
//! de tous les listings. L'utilisateur peut le restaurer pendant le
//! délai de rétention (configurable, 1h pour les tests, 7j en prod).
//!
//! Le mot de confirmation est une protection UX contre les clics
//! accidentels — la sécurité repose sur l'auth JWT + ownership check.

use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::repository::Repository;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::repo_repository::RepoRepository;

// ── Commands ──────────────────────────────────────────────────────────

/// Commande de soft delete (mise en corbeille).
pub struct SoftDeleteCommand {
    /// Acteur qui demande la suppression.
    pub actor_id: Uuid,
    /// Handle du propriétaire du dépôt.
    pub owner: String,
    /// Nom slug du dépôt.
    pub repo: String,
    /// Mot de confirmation tapé par l'utilisateur.
    pub confirmation_word: String,
    /// Mot de confirmation attendu (généré côté frontend).
    pub expected_word: String,
}

/// Commande de restauration depuis la corbeille.
pub struct RestoreCommand {
    /// Acteur qui demande la restauration.
    pub actor_id: Uuid,
    /// Handle du propriétaire du dépôt.
    pub owner: String,
    /// Nom slug du dépôt.
    pub repo: String,
}

// ── Use Case ──────────────────────────────────────────────────────────

/// Gère la suppression et la restauration des dépôts.
pub struct DeleteRepositoryUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl DeleteRepositoryUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self {
            actor_repo,
            repo_repo,
        }
    }

    /// Met un dépôt en corbeille (soft delete).
    ///
    /// Vérifie :
    /// 1. Le mot de confirmation correspond
    /// 2. L'acteur est le propriétaire du dépôt
    #[instrument(skip(self, cmd), fields(actor_id = %cmd.actor_id, owner = %cmd.owner, repo = %cmd.repo))]
    pub async fn execute_soft_delete(&self, cmd: SoftDeleteCommand) -> Result<(), DomainError> {
        // 1. Vérifier le mot de confirmation
        if cmd.confirmation_word != cmd.expected_word {
            return Err(DomainError::BusinessRule(
                "Le mot de confirmation ne correspond pas".to_string(),
            ));
        }

        // 2. Résoudre le propriétaire
        let actor = self
            .actor_repo
            .find_by_handle(&cmd.owner)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        // 3. Vérifier l'ownership
        if actor.id != cmd.actor_id {
            return Err(DomainError::BusinessRule(
                "Seul le propriétaire peut supprimer un dépôt".to_string(),
            ));
        }

        // 4. Trouver le repo
        let repo = self
            .repo_repo
            .find_by_owner_and_name(&actor.id, &cmd.repo)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: Uuid::nil(),
            })?;

        // 5. Soft delete
        let deleted = self.repo_repo.soft_delete(&repo.id).await?;
        if !deleted {
            warn!(
                repo_id = %repo.id,
                "Soft delete échoué — repo déjà en corbeille ?"
            );
        } else {
            info!(
                repo_id = %repo.id,
                repo_name = %repo.name,
                owner = %cmd.owner,
                "🗑️ Dépôt mis en corbeille (soft delete)"
            );
        }

        Ok(())
    }

    /// Restaure un dépôt depuis la corbeille.
    ///
    /// Vérifie que l'acteur est le propriétaire.
    #[instrument(skip(self, cmd), fields(actor_id = %cmd.actor_id, owner = %cmd.owner, repo = %cmd.repo))]
    pub async fn execute_restore(&self, cmd: RestoreCommand) -> Result<Repository, DomainError> {
        // 1. Résoudre le propriétaire
        let actor = self
            .actor_repo
            .find_by_handle(&cmd.owner)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        // 2. Vérifier l'ownership
        if actor.id != cmd.actor_id {
            return Err(DomainError::BusinessRule(
                "Seul le propriétaire peut restaurer un dépôt".to_string(),
            ));
        }

        // 3. Trouver le repo dans la corbeille
        let trash_repos = self.repo_repo.list_deleted_by_owner(&actor.id).await?;
        let repo = trash_repos
            .into_iter()
            .find(|r| r.name == cmd.repo)
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository (trash)",
                id: Uuid::nil(),
            })?;

        // 4. Restaurer
        let restored = self.repo_repo.restore(&repo.id).await?;
        if !restored {
            warn!(repo_id = %repo.id, "Restauration échouée — repo déjà actif ?");
        } else {
            info!(
                repo_id = %repo.id,
                repo_name = %repo.name,
                owner = %cmd.owner,
                "♻️ Dépôt restauré depuis la corbeille"
            );
        }

        // 5. Re-lire le repo restauré
        self.repo_repo
            .find_by_id(&repo.id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: repo.id,
            })
    }

    /// Liste les dépôts en corbeille d'un acteur.
    #[instrument(skip(self), fields(handle = %handle))]
    pub async fn list_trash(&self, handle: &str) -> Result<Vec<Repository>, DomainError> {
        let actor = self
            .actor_repo
            .find_by_handle(handle)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        self.repo_repo.list_deleted_by_owner(&actor.id).await
    }
}
