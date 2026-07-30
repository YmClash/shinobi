//! Modèles de données ANBU — Types centraux pour les checkpoints et artifacts.
//!
//! Ces structures représentent le contexte IA capturé par ANBU :
//! - `Checkpoint` : un snapshot figé du contexte IA à un instant donné
//! - `Artifact` : un fichier individuel capturé (plan, task, walkthrough, log)
//! - `AgentKind` : le type d'agent IA source (Antigravity V1)

use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Checkpoint ───────────────────────────────────────────────────────────

/// Un checkpoint ANBU — snapshot du contexte IA figé dans le temps.
///
/// Chaque checkpoint est associé à un commit jj/git via des trailers
/// et contient une liste d'artifacts capturés depuis une session IA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Identifiant unique du checkpoint.
    pub id: Uuid,
    /// Type d'agent IA source.
    pub agent: AgentKind,
    /// Identifiant de la session IA (ex: conversation-id Antigravity).
    pub session_id: String,
    /// Description utilisateur du checkpoint.
    pub message: Option<String>,
    /// SHA du commit jj/git associé (rempli après `jj describe`).
    pub commit_id: Option<String>,
    /// Chemin absolu du repo où le checkpoint a été créé.
    pub repo_path: String,
    /// Artifacts capturés.
    pub artifacts: Vec<Artifact>,
    /// Date de création.
    pub created_at: DateTime<Utc>,
}

// ── Artifact ─────────────────────────────────────────────────────────────

/// Un artifact capturé depuis une session IA.
///
/// Chaque artifact est un fichier (plan, task, walkthrough, log) copié
/// depuis le répertoire de l'agent vers le staging local `.shinobi/anbu/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Identifiant unique.
    pub id: Uuid,
    /// Type sémantique de l'artifact.
    pub kind: ArtifactKind,
    /// Nom du fichier original (ex: `implementation_plan.md`).
    pub filename: String,
    /// Chemin original complet (dans le brain/).
    pub source_path: PathBuf,
    /// Chemin de stockage dans `.shinobi/anbu/checkpoints/{uuid}/`.
    pub stored_path: PathBuf,
    /// Hash SHA-256 du contenu (pour déduplication).
    pub content_hash: String,
    /// Taille en octets.
    pub size_bytes: u64,
    /// Date de capture.
    pub captured_at: DateTime<Utc>,
}

// ── AgentKind ────────────────────────────────────────────────────────────

/// Type d'agent IA supporté par ANBU.
///
/// V1 supporte uniquement Antigravity (Gemini).
/// Le trait `Collector` permettra d'ajouter d'autres agents en V2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Gemini / Antigravity (Google DeepMind).
    Antigravity,
    // Future V2:
    // Cursor,
    // Copilot,
    // Claude,
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentKind::Antigravity => write!(f, "antigravity"),
        }
    }
}

impl std::str::FromStr for AgentKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "antigravity" | "gemini" => Ok(AgentKind::Antigravity),
            _ => Err(anyhow::anyhow!("Unknown agent: {s}. Supported: antigravity")),
        }
    }
}

// ── ArtifactKind ─────────────────────────────────────────────────────────

/// Type sémantique d'un artifact capturé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Plan d'implémentation (`implementation_plan.md`).
    ImplementationPlan,
    /// Liste de tâches (`task.md`).
    Task,
    /// Résumé post-implémentation (`walkthrough.md`).
    Walkthrough,
    /// Log de conversation (`overview.txt`).
    ConversationLog,
    /// Fichier média (screenshot, recording).
    Media,
    /// Tout autre fichier.
    Other,
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactKind::ImplementationPlan => write!(f, "📋 plan"),
            ArtifactKind::Task => write!(f, "✅ task"),
            ArtifactKind::Walkthrough => write!(f, "📝 walkthrough"),
            ArtifactKind::ConversationLog => write!(f, "💬 log"),
            ArtifactKind::Media => write!(f, "🖼️ media"),
            ArtifactKind::Other => write!(f, "📄 other"),
        }
    }
}

impl ArtifactKind {
    /// Détecte le type d'artifact à partir du nom de fichier.
    pub fn from_filename(filename: &str) -> Self {
        let lower = filename.to_lowercase();
        if lower.contains("implementation_plan") {
            ArtifactKind::ImplementationPlan
        } else if lower == "task.md" {
            ArtifactKind::Task
        } else if lower.contains("walkthrough") {
            ArtifactKind::Walkthrough
        } else if lower == "overview.txt" {
            ArtifactKind::ConversationLog
        } else if lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".webp")
            || lower.ends_with(".mp4")
        {
            ArtifactKind::Media
        } else {
            ArtifactKind::Other
        }
    }
}

// ── SessionInfo ──────────────────────────────────────────────────────────

/// Information sur une session IA détectée par un collecteur.
///
/// Utilisé par `anbu sessions` pour lister les sessions disponibles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Identifiant de la session (conversation-id pour Antigravity).
    pub id: String,
    /// Agent source.
    pub agent: AgentKind,
    /// Résumé de la session (extrait des métadonnées).
    pub summary: Option<String>,
    /// Date de dernière modification.
    pub last_modified: DateTime<Utc>,
    /// Nombre d'artifacts détectés.
    pub artifact_count: usize,
    /// Chemin du répertoire de la session.
    pub path: PathBuf,
}
