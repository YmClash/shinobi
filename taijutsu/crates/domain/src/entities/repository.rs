//! Entité Repository — Dépôt de code versionné dans la Forge Sociale.
//!
//! Un Repository est l'unité d'isolation multi-tenant de SHINOBI.
//! Chaque dépôt possède son propre workspace VCS (Jujutsu), son propre
//! graphe d'opérations, et ses propres collaborateurs.
//!
//! ## Propriété par les Acteurs
//! Un dépôt peut être possédé par n'importe quel type d'acteur :
//! - Un humain qui développe son projet
//! - Un agent IA qui fork pour expérimenter (Laboratoire Autonome)
//! - Le système pour le dépôt par défaut (migration MVP)
//!
//! ## Isolation
//! Chaque repository correspond à un workspace Jujutsu physique
//! dans `/app/workspace/{repo_id}/.jj`. Le `JujutsuEngine` maintient
//! un registre `DashMap<Uuid, Arc<Mutex<WorkspaceHandle>>>` pour
//! la concurrence par-repo.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Visibilité ────────────────────────────────────────────────────────

/// Visibilité d'un dépôt dans la Forge Sociale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Visible par tous les acteurs (listing public, ActivityPub).
    Public,
    /// Visible uniquement par les collaborateurs du dépôt.
    Private,
}

impl Visibility {
    /// Représentation SQL compatible avec le type ENUM PostgreSQL.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
        }
    }

    /// Parse depuis une chaîne SQL.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Visibility::Public),
            "private" => Some(Visibility::Private),
            _ => None,
        }
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── Entité Repository ─────────────────────────────────────────────────

/// Dépôt de code versionné — unité d'isolation multi-tenant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repository {
    /// Identifiant unique du dépôt.
    pub id: Uuid,

    /// Identifiant du propriétaire (Actor — humain, IA, ou système).
    pub owner_id: Uuid,

    /// Nom slug unique par propriétaire (ex: "shinobi", "my-lib").
    /// Utilisé dans les URLs : `/{owner_handle}/{name}`
    pub name: String,

    /// Nom d'affichage lisible (ex: "Shinobi VCS").
    pub display_name: String,

    /// Description du dépôt (optionnel).
    pub description: Option<String>,

    /// Visibilité du dépôt (Public ou Private).
    pub visibility: Visibility,

    /// Branche par défaut (ex: "main").
    pub default_branch: String,

    /// Date de création du dépôt.
    pub created_at: DateTime<Utc>,

    // ── Phase 19B — GitHub Import ─────────────────────────────────

    /// URL Git source pour les repos importés (ex: "https://github.com/user/repo.git").
    /// `None` = repo natif SHINOBI.
    pub mirror_source_url: Option<String>,

    /// Timestamp du dernier import miroir réussi.
    /// `None` = jamais synchronisé ou repo natif.
    pub mirror_synced_at: Option<DateTime<Utc>>,

    // ── Phase 24 — Soft Delete ────────────────────────────────────

    /// Timestamp de suppression (soft delete). `None` = dépôt actif.
    /// Si renseigné, le repo est en corbeille et sera purgé après le délai de rétention.
    pub deleted_at: Option<DateTime<Utc>>,

    // ── Phase 37B — Fork Local (Le Dédoublement) ─────────────────

    /// UUID du dépôt parent si ce repo est un fork. `None` = repo original.
    /// FK vers `repositories(id)` avec `ON DELETE SET NULL` — si le parent
    /// est supprimé, le fork survit en tant que repo autonome.
    pub forked_from_id: Option<Uuid>,
}

impl Repository {
    /// Construit un nouveau dépôt public.
    pub fn new(
        owner_id: Uuid,
        name: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            owner_id,
            name: name.into(),
            display_name: display_name.into(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_string(),
            created_at: Utc::now(),
            mirror_source_url: None,
            mirror_synced_at: None,
            deleted_at: None,
            forked_from_id: None,
        }
    }

    /// Vérifie si le dépôt est public.
    pub fn is_public(&self) -> bool {
        self.visibility == Visibility::Public
    }

    /// Vérifie si le dépôt est privé.
    pub fn is_private(&self) -> bool {
        self.visibility == Visibility::Private
    }

    /// Vérifie si le dépôt est un miroir importé depuis GitHub (Phase 19B).
    pub fn is_mirror(&self) -> bool {
        self.mirror_source_url.is_some()
    }

    /// Vérifie si le dépôt est dans la corbeille (soft-deleted).
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Vérifie si le dépôt est un fork d'un autre dépôt (Phase 37B).
    pub fn is_fork(&self) -> bool {
        self.forked_from_id.is_some()
    }

    /// Nombre de secondes restantes avant purge définitive.
    /// Retourne `None` si le dépôt n'est pas supprimé.
    pub fn seconds_until_purge(&self, retention_secs: i64) -> Option<i64> {
        self.deleted_at.map(|d| {
            let deadline = d + chrono::Duration::seconds(retention_secs);
            (deadline - Utc::now()).num_seconds().max(0)
        })
    }
}

// ── Rôle de collaborateur ─────────────────────────────────────────────

/// Rôle d'un collaborateur dans un dépôt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoRole {
    /// Propriétaire — tous les droits, y compris suppression.
    Owner,
    /// Mainteneur — merge, review, configuration.
    Maintainer,
    /// Contributeur — créer des opérations (commits/PRs).
    Contributor,
    /// Observateur — lecture seule.
    Viewer,
}

impl RepoRole {
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            RepoRole::Owner => "owner",
            RepoRole::Maintainer => "maintainer",
            RepoRole::Contributor => "contributor",
            RepoRole::Viewer => "viewer",
        }
    }

    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(RepoRole::Owner),
            "maintainer" => Some(RepoRole::Maintainer),
            "contributor" => Some(RepoRole::Contributor),
            "viewer" => Some(RepoRole::Viewer),
            _ => None,
        }
    }
}

// ── Tests unitaires ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_new_defaults() {
        let repo = Repository::new(Uuid::new_v4(), "my-lib", "My Library");
        assert!(repo.is_public());
        assert!(!repo.is_private());
        assert_eq!(repo.default_branch, "main");
        assert!(repo.description.is_none());
    }

    #[test]
    fn test_repository_unique_ids() {
        let owner = Uuid::new_v4();
        let r1 = Repository::new(owner, "repo-a", "Repo A");
        let r2 = Repository::new(owner, "repo-b", "Repo B");
        assert_ne!(r1.id, r2.id);
    }

    #[test]
    fn test_visibility_sql_roundtrip() {
        for vis in [Visibility::Public, Visibility::Private] {
            let sql = vis.as_sql_str();
            let parsed = Visibility::from_sql_str(sql).unwrap();
            assert_eq!(vis, parsed);
        }
    }

    #[test]
    fn test_repo_role_sql_roundtrip() {
        for role in [RepoRole::Owner, RepoRole::Maintainer, RepoRole::Contributor, RepoRole::Viewer] {
            let sql = role.as_sql_str();
            let parsed = RepoRole::from_sql_str(sql).unwrap();
            assert_eq!(role, parsed);
        }
    }

    #[test]
    fn test_repository_serde_roundtrip() {
        let repo = Repository::new(Uuid::new_v4(), "shinobi", "Shinobi VCS");
        let json = serde_json::to_string(&repo).unwrap();
        let deserialized: Repository = serde_json::from_str(&json).unwrap();
        assert_eq!(repo, deserialized);
    }
}
