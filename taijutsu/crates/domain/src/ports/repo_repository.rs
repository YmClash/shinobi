//! Port: RepoRepository — Contrat de persistence des dépôts.
//!
//! Ce trait définit le contrat pour sauvegarder et retrouver des dépôts
//! de code versionné dans la Forge Sociale multi-tenant.

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::repository::Repository;
use crate::errors::DomainError;

/// Contrat de persistence pour les dépôts de la Forge Sociale.
///
/// Implémenté par `PostgresRepoRepository` dans la couche infrastructure.
#[async_trait]
pub trait RepoRepository: Send + Sync {
    /// Persiste un nouveau dépôt.
    async fn save(&self, repo: &Repository) -> Result<(), DomainError>;

    /// Retrouve un dépôt par son identifiant.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Repository>, DomainError>;

    /// Retrouve un dépôt par son propriétaire et son nom slug.
    /// Contrainte UNIQUE (owner_id, name) en base.
    async fn find_by_owner_and_name(
        &self,
        owner_id: &Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError>;

    /// Liste tous les dépôts d'un propriétaire.
    async fn list_by_owner(&self, owner_id: &Uuid) -> Result<Vec<Repository>, DomainError>;

    /// Liste les dépôts publics (exploration / landing page).
    async fn list_public(&self, limit: usize) -> Result<Vec<Repository>, DomainError>;

    /// Ajoute un collaborateur au dépôt avec un rôle donné.
    /// Idempotent : `ON CONFLICT DO NOTHING` côté PostgreSQL.
    async fn add_collaborator(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
        role: &str,
    ) -> Result<(), DomainError>;

    /// Vérifie si un acteur est collaborateur d'un dépôt (Phase 19A — RBAC).
    async fn is_collaborator(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
    ) -> Result<bool, DomainError>;

    /// Retourne le rôle d'un acteur dans un dépôt (Phase 19A — RBAC).
    /// Retourne None si l'acteur n'est pas collaborateur.
    async fn get_role(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
    ) -> Result<Option<String>, DomainError>;
}
