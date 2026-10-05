//! Entité CommitStatus — Phase 39 (Le Pont CI/CD) 🌉
//!
//! Représente le statut d'un commit reporté par un pipeline CI/CD externe.
//! Permet aux développeurs de voir le résultat des builds directement
//! dans Shinobi (badges 🟢🔴🟡 à côté du hash).
//!
//! ## Sémantique
//! - Un commit peut avoir **plusieurs** statuts (un par `context`).
//! - Le `context` est le nom du pipeline (ex: "drone/build", "woodpecker/test").
//! - Le **combined status** agrège tous les statuts d'un commit :
//!   - `success` si tous sont `success`
//!   - `pending` si ≥1 est `pending` (et aucun `failure`/`error`)
//!   - `failure` sinon
//!
//! ## UPSERT
//! Un POST avec le même `(repository_id, commit_id, context)` met à jour
//! le statut existant (pas de duplication fantôme).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── CommitStatusState ─────────────────────────────────────────────────

/// État d'un statut de commit CI/CD.
///
/// Suit la convention GitHub/GitLab :
/// - `Pending` : build en cours ou non démarré
/// - `Success` : build réussi ✅
/// - `Failure` : build échoué ❌
/// - `Error`   : erreur infrastructure (timeout, OOM, etc.) ⚠️
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitStatusState {
    Pending,
    Success,
    Failure,
    Error,
}

impl CommitStatusState {
    /// Convertit depuis la valeur SQL VARCHAR.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// Convertit vers la valeur SQL VARCHAR.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for CommitStatusState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── CommitStatus ──────────────────────────────────────────────────────

/// Statut d'un commit reporté par un pipeline CI/CD externe.
///
/// Clé d'unicité : `(repository_id, commit_id, context)`.
/// Un POST avec la même clé déclenche un UPSERT (mise à jour).
#[derive(Debug, Clone)]
pub struct CommitStatus {
    /// Identifiant unique.
    pub id: Uuid,
    /// Dépôt associé.
    pub repository_id: Uuid,
    /// SHA du commit (git) ou change ID (jujutsu).
    pub commit_id: String,
    /// Identifiant du pipeline CI (ex: "drone/build", "woodpecker/test").
    pub context: String,
    /// État du build.
    pub state: CommitStatusState,
    /// Description courte (ex: "Build passed in 42s").
    pub description: Option<String>,
    /// URL vers le dashboard CI externe (Drone, Jenkins, etc.).
    pub target_url: Option<String>,
    /// Acteur ayant créé le statut (Service Account ou humain).
    pub creator_id: Option<Uuid>,
    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour.
    pub updated_at: DateTime<Utc>,
}

impl CommitStatus {
    /// Construit un nouveau commit status.
    pub fn new(
        repository_id: Uuid,
        commit_id: String,
        context: String,
        state: CommitStatusState,
        description: Option<String>,
        target_url: Option<String>,
        creator_id: Option<Uuid>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repository_id,
            commit_id,
            context,
            state,
            description,
            target_url,
            creator_id,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── CombinedStatus ────────────────────────────────────────────────────

/// Statut combiné d'un commit (agrégation de tous les statuts).
///
/// Logique GitHub-style :
/// - `Success` si **tous** les statuts sont `Success`
/// - `Pending` si ≥1 est `Pending` (et aucun `Failure`/`Error`)
/// - `Failure` sinon (≥1 est `Failure` ou `Error`)
#[derive(Debug, Clone)]
pub struct CombinedStatus {
    /// État combiné agrégé.
    pub state: CommitStatusState,
    /// Nombre total de statuts.
    pub total_count: usize,
    /// Détail de chaque statut individuel.
    pub statuses: Vec<CommitStatus>,
}

impl CombinedStatus {
    /// Calcule le statut combiné à partir d'une liste de statuts.
    pub fn from_statuses(statuses: Vec<CommitStatus>) -> Self {
        let total_count = statuses.len();

        let state = if total_count == 0 {
            // Aucun statut → considéré comme pending (pas de données)
            CommitStatusState::Pending
        } else if statuses.iter().all(|s| s.state == CommitStatusState::Success) {
            CommitStatusState::Success
        } else if statuses.iter().any(|s| s.state == CommitStatusState::Failure || s.state == CommitStatusState::Error) {
            CommitStatusState::Failure
        } else {
            // Il y a au moins un Pending (et pas de failure/error)
            CommitStatusState::Pending
        };

        Self {
            state,
            total_count,
            statuses,
        }
    }
}
