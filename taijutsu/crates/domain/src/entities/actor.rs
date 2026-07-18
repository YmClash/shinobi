//! Entité Actor — Représente un acteur du système SHINOBI.
//!
//! Un Actor est l'unité d'identité fondamentale de la Forge Sociale.
//! Il peut être un développeur humain, un agent IA autonome (Tensai, Oracle,
//! OpenHands, Devin...) ou le système lui-même.
//!
//! ## Alignement ActivityPub (Phase 10C)
//! Le concept d'Actor est aligné nativement sur le protocole W3C ActivityPub :
//! - `Human` → ActivityPub `Person`
//! - `AiAgent` → ActivityPub `Service` / `Application`
//! - `System` → ActivityPub `Service` (instance-level)
//!
//! ## Namespace Universel
//! Le `handle` est unique tous types confondus. Un agent IA `@oracle`
//! et un humain ne peuvent pas partager le même handle — protection
//! contre les attaques par usurpation d'identité IA.
//!
//! ## Constantes Fantômes
//! `SYSTEM_ACTOR_ID` et `DEFAULT_REPO_ID` sont des UUIDs déterministes
//! utilisés dans la migration SQL pour rattacher les données MVP existantes.
//! Ils DOIVENT rester synchronisés avec `006_actors_repositories.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Constantes Fantômes (synchronisées avec migration 006) ────────────

/// UUID déterministe de l'acteur système SHINOBI.
/// Propriétaire du dépôt par défaut et rattachement des opérations MVP.
///
/// Valeur : `00000000-0000-0000-0000-000000000001`
///
/// ⚠️ DOIT correspondre à la valeur dans `migrations/006_actors_repositories.sql`.
pub const SYSTEM_ACTOR_ID: Uuid = Uuid::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

/// UUID déterministe du dépôt par défaut.
/// Les opérations MVP existantes y sont rattachées lors de la migration.
///
/// Valeur : `00000000-0000-0000-0000-000000000002`
///
/// ⚠️ DOIT correspondre à la valeur dans `migrations/006_actors_repositories.sql`.
pub const DEFAULT_REPO_ID: Uuid = Uuid::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);

// ── Type d'Acteur ─────────────────────────────────────────────────────

/// Type d'un acteur dans le système SHINOBI.
///
/// Détermine les capacités d'authentification et les workflows disponibles.
/// Un `AiAgent` peut posséder des dépôts, forker, et soumettre des opérations
/// au même titre qu'un `Human` — citoyens de première classe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    /// Développeur humain — s'authentifie via email/password, OAuth, Passkeys.
    Human,
    /// Agent IA autonome — s'authentifie via API Key, mTLS, JWT signé.
    /// Exemples : Tensai, Oracle, OpenHands, Devin.
    AiAgent,
    /// Acteur système interne — pas d'authentification externe.
    /// Utilisé pour les opérations de migration et le dépôt par défaut.
    System,
}

impl ActorType {
    /// Représentation SQL compatible avec le type ENUM PostgreSQL.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            ActorType::Human => "human",
            ActorType::AiAgent => "ai_agent",
            ActorType::System => "system",
        }
    }

    /// Parse depuis une chaîne SQL.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "human" => Some(ActorType::Human),
            "ai_agent" => Some(ActorType::AiAgent),
            "system" => Some(ActorType::System),
            _ => None,
        }
    }
}

impl std::fmt::Display for ActorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── Entité Actor ──────────────────────────────────────────────────────

/// Acteur du système — humain, agent IA, ou système.
///
/// Remplace l'ancienne entité `User` avec un modèle Actor-first
/// aligné sur le protocole W3C ActivityPub.
///
/// ## Citoyens de Première Classe
/// Un `AiAgent` possède les mêmes droits qu'un `Human` :
/// - Créer et posséder des dépôts
/// - Soumettre des opérations (commits)
/// - Forker des dépôts d'autres acteurs
/// - Recevoir des opérations (Pull Requests)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Actor {
    /// Identifiant unique.
    pub id: Uuid,

    /// Handle unique universel (ex: "@tensai", "@oracle", "@dev-alice").
    /// Namespace partagé entre tous les types d'acteurs — empêche
    /// l'usurpation d'identité cross-type.
    pub handle: String,

    /// Nom d'affichage lisible.
    pub display_name: String,

    /// Type d'acteur (Human, AiAgent, System).
    pub actor_type: ActorType,

    /// URL de l'avatar (optionnel).
    pub avatar_url: Option<String>,

    /// Adresse email (optionnel — humains uniquement, login Phase 19A).
    pub email: Option<String>,

    /// Biographie / description (optionnel).
    pub bio: Option<String>,

    /// Identifiant GitHub (OAuth Phase 20). Immuable, unique.
    pub github_id: Option<i64>,

    /// Token OAuth GitHub en clair (Phase 20B — Le Clonage Massif).
    /// Mis à jour à chaque login OAuth. Permet les appels API GitHub
    /// authentifiés (lister les repos de l'utilisateur, etc.).
    pub github_token: Option<String>,

    /// Date de création du compte.
    pub created_at: DateTime<Utc>,
}

impl Actor {
    /// Construit un nouvel acteur.
    pub fn new(
        handle: impl Into<String>,
        display_name: impl Into<String>,
        actor_type: ActorType,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            handle: handle.into(),
            display_name: display_name.into(),
            actor_type,
            avatar_url: None,
            email: None,
            bio: None,
            github_id: None,
            github_token: None,
            created_at: Utc::now(),
        }
    }

    /// Vérifie si cet acteur est le système SHINOBI.
    pub fn is_system(&self) -> bool {
        self.id == SYSTEM_ACTOR_ID
    }

    /// Vérifie si cet acteur est un agent IA.
    pub fn is_ai(&self) -> bool {
        self.actor_type == ActorType::AiAgent
    }

    /// Vérifie si cet acteur est un humain.
    pub fn is_human(&self) -> bool {
        self.actor_type == ActorType::Human
    }
}

// ── Tests unitaires ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_new_generates_unique_id() {
        let a1 = Actor::new("alice", "Alice", ActorType::Human);
        let a2 = Actor::new("bob", "Bob", ActorType::Human);
        assert_ne!(a1.id, a2.id);
    }

    #[test]
    fn test_actor_type_human() {
        let actor = Actor::new("alice", "Alice Dev", ActorType::Human);
        assert!(actor.is_human());
        assert!(!actor.is_ai());
        assert!(!actor.is_system());
    }

    #[test]
    fn test_actor_type_ai_agent() {
        let actor = Actor::new("tensai", "Tensai Agent", ActorType::AiAgent);
        assert!(actor.is_ai());
        assert!(!actor.is_human());
        assert!(!actor.is_system());
    }

    #[test]
    fn test_actor_type_system() {
        let mut actor = Actor::new("system", "SHINOBI System", ActorType::System);
        actor.id = SYSTEM_ACTOR_ID;
        assert!(actor.is_system());
        assert!(!actor.is_human());
        assert!(!actor.is_ai());
    }

    #[test]
    fn test_system_actor_id_constant() {
        assert_eq!(
            SYSTEM_ACTOR_ID.to_string(),
            "00000000-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn test_default_repo_id_constant() {
        assert_eq!(
            DEFAULT_REPO_ID.to_string(),
            "00000000-0000-0000-0000-000000000002"
        );
    }

    #[test]
    fn test_actor_type_sql_roundtrip() {
        for actor_type in [ActorType::Human, ActorType::AiAgent, ActorType::System] {
            let sql = actor_type.as_sql_str();
            let parsed = ActorType::from_sql_str(sql).unwrap();
            assert_eq!(actor_type, parsed);
        }
    }

    #[test]
    fn test_actor_serde_roundtrip() {
        let actor = Actor::new("oracle", "Oracle Reviewer", ActorType::AiAgent);
        let json = serde_json::to_string(&actor).unwrap();
        let deserialized: Actor = serde_json::from_str(&json).unwrap();
        assert_eq!(actor, deserialized);
    }

    #[test]
    fn test_actor_type_serde_snake_case() {
        let json = serde_json::to_string(&ActorType::AiAgent).unwrap();
        assert_eq!(json, "\"ai_agent\"");
    }
}
