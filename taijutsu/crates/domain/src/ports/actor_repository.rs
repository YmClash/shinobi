//! Port: ActorRepository — Contrat de persistence des acteurs.
//!
//! Ce trait définit le contrat pour sauvegarder et retrouver des acteurs
//! (humains, agents IA, système). L'implémentation concrète vit dans
//! infrastructure/ (Fūinjutsu / PostgreSQL).

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::actor::Actor;
use crate::errors::DomainError;

/// Contrat de persistence pour les acteurs de la Forge Sociale.
///
/// Implémenté par `PostgresActorRepository` dans la couche infrastructure.
#[async_trait]
pub trait ActorRepository: Send + Sync {
    /// Persiste un nouvel acteur.
    async fn save(&self, actor: &Actor) -> Result<(), DomainError>;

    /// Retrouve un acteur par son identifiant.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Actor>, DomainError>;

    /// Retrouve un acteur par son handle unique.
    /// Le handle est unique tous types confondus (namespace universel).
    async fn find_by_handle(&self, handle: &str) -> Result<Option<Actor>, DomainError>;

    /// Retrouve un acteur par son email (login Phase 19A).
    /// Cherche dans la table credentials (email) et retourne l'acteur associé.
    async fn find_by_email(&self, email: &str) -> Result<Option<Actor>, DomainError>;

    // ── Credentials (Phase 19A — Auth) ─────────────────────────

    /// Stocke un credential pour un acteur.
    /// Types: "password", "api_key", "oauth_token", "ssh_key".
    async fn save_credential(
        &self,
        actor_id: &Uuid,
        cred_type: &str,
        secret_hash: &str,
        email: Option<&str>,
        label: Option<&str>,
    ) -> Result<(), DomainError>;

    /// Retrouve le hash du credential de type donné pour un acteur.
    /// Retourne le `secret_hash` ou None si aucun credential de ce type.
    async fn find_credential_hash(
        &self,
        actor_id: &Uuid,
        cred_type: &str,
    ) -> Result<Option<String>, DomainError>;

    /// Retrouve tous les hashes de credentials de type donné pour un acteur.
    /// Utilisé pour les PAT (un acteur peut avoir N api_keys).
    async fn find_all_credential_hashes(
        &self,
        actor_id: &Uuid,
        cred_type: &str,
    ) -> Result<Vec<String>, DomainError>;

    /// Retrouve un acteur par le hash de son credential (reverse lookup).
    ///
    /// Utilisé par le Git HTTP Bridge pour authentifier les PAT via Basic Auth :
    /// le PAT brut est hashé (SHA-256) puis recherché dans la table credentials
    /// pour retrouver l'acteur propriétaire.
    ///
    /// ## Arguments
    /// - `secret_hash` : hash SHA-256 hex du token brut
    /// - `cred_type` : type de credential (ex: `"api_key"` pour les PAT)
    async fn find_actor_by_credential_hash(
        &self,
        secret_hash: &str,
        cred_type: &str,
    ) -> Result<Option<Actor>, DomainError>;

    /// Retrouve un acteur par son identifiant GitHub OAuth (Phase 20).
    async fn find_by_github_id(&self, github_id: i64) -> Result<Option<Actor>, DomainError>;

    /// Lie un compte GitHub à un acteur existant (Phase 20).
    async fn update_github_id(&self, actor_id: &Uuid, github_id: i64) -> Result<(), DomainError>;

    /// Stocke le token OAuth GitHub en clair (Phase 20B — Le Clonage Massif).
    /// Mis à jour à chaque login OAuth pour rester frais.
    async fn update_github_token(&self, actor_id: &Uuid, token: &str) -> Result<(), DomainError>;

    /// Récupère le token OAuth GitHub d'un acteur (Phase 20B).
    /// Retourne `None` si l'acteur n'a pas lié son compte GitHub.
    async fn get_github_token(&self, actor_id: &Uuid) -> Result<Option<String>, DomainError>;

    /// Liste les PAT d'un acteur (label + created_at, pas le hash).
    async fn list_pats(
        &self,
        actor_id: &Uuid,
    ) -> Result<Vec<PatInfo>, DomainError>;
}

/// Informations d'un PAT (sans le secret).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PatInfo {
    pub id: Uuid,
    pub label: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
