//! Entités Pipeline & PipelineStage — Phase 40 (Jutsu Runner Natif) 🥷⚡
//!
//! Représentent un pipeline CI/CD natif et ses étapes d'exécution.
//!
//! ## Sémantique
//! - Un **Pipeline** est une exécution complète d'un `jutsu.yml` pour un commit donné.
//! - Chaque pipeline contient N **PipelineStages** (les étapes, exécutées séquentiellement en V1).
//! - Le `trigger_event` indique ce qui a déclenché le pipeline (push, mr_created, tag, manual).
//! - Le `status` du pipeline est calculé dynamiquement à partir des statuts des stages.
//!
//! ## Workflow
//! ```text
//! Pipeline: queued → running → success/failure/error/cancelled
//! Stage:    pending → running → success/failure/error/skipped
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── PipelineStatus ──────────────────────────────────────────────────

/// Statut global d'un pipeline CI/CD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    /// En attente d'exécution (dans la file Kafka).
    Queued,
    /// En cours d'exécution (au moins un stage a démarré).
    Running,
    /// Tous les stages ont réussi ✅.
    Success,
    /// Au moins un stage a échoué ❌.
    Failure,
    /// Erreur infrastructure (Docker crash, timeout, etc.) ⚠️.
    Error,
    /// Annulé manuellement.
    Cancelled,
}

impl PipelineStatus {
    /// Convertit depuis la valeur SQL VARCHAR.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            "error" => Some(Self::Error),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Convertit vers la valeur SQL VARCHAR.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── PipelineStageStatus ─────────────────────────────────────────────

/// Statut d'une étape individuelle d'un pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStageStatus {
    /// En attente (dépendances pas encore satisfaites ou pas encore lancé).
    Pending,
    /// En cours d'exécution dans le container Docker.
    Running,
    /// Le container s'est terminé avec exit code 0 ✅.
    Success,
    /// Le container s'est terminé avec exit code ≠ 0 ❌.
    Failure,
    /// Erreur infrastructure (image introuvable, Docker crash, timeout) ⚠️.
    Error,
    /// Ignoré car une dépendance a échoué.
    Skipped,
}

impl PipelineStageStatus {
    /// Convertit depuis la valeur SQL VARCHAR.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            "error" => Some(Self::Error),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    /// Convertit vers la valeur SQL VARCHAR.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

impl std::fmt::Display for PipelineStageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── TriggerEvent ────────────────────────────────────────────────────

/// Événement déclencheur d'un pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerEvent {
    /// Déclenché par un git push.
    Push,
    /// Déclenché par la création d'une Merge Request.
    MrCreated,
    /// Déclenché par la création d'un tag.
    Tag,
    /// Déclenché manuellement (API REST ou CLI).
    Manual,
}

impl TriggerEvent {
    /// Convertit depuis la valeur SQL VARCHAR.
    pub fn from_sql(s: &str) -> Option<Self> {
        match s {
            "push" => Some(Self::Push),
            "mr_created" => Some(Self::MrCreated),
            "tag" => Some(Self::Tag),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    /// Convertit vers la valeur SQL VARCHAR.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::MrCreated => "mr_created",
            Self::Tag => "tag",
            Self::Manual => "manual",
        }
    }
}

impl std::fmt::Display for TriggerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── Pipeline ────────────────────────────────────────────────────────

/// Pipeline CI/CD natif — une exécution complète du `jutsu.yml`.
///
/// Chaque push/MR/tag peut déclencher un pipeline qui orchestre
/// l'exécution séquentielle des stages dans des containers Docker.
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Identifiant unique.
    pub id: Uuid,
    /// Dépôt associé.
    pub repository_id: Uuid,
    /// SHA du commit Git à builder.
    pub commit_id: String,
    /// Événement déclencheur.
    pub trigger_event: TriggerEvent,
    /// Statut global du pipeline.
    pub status: PipelineStatus,
    /// Nom du pipeline (depuis `jutsu.yml` → `name:`).
    pub pipeline_name: Option<String>,
    /// Début d'exécution.
    pub started_at: Option<DateTime<Utc>>,
    /// Fin d'exécution.
    pub finished_at: Option<DateTime<Utc>>,
    /// Durée totale en millisecondes.
    pub duration_ms: Option<i32>,
    /// Acteur ayant déclenché le pipeline (humain ou service account).
    pub creator_id: Option<Uuid>,
    /// Date de création en base.
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour.
    pub updated_at: DateTime<Utc>,
}

impl Pipeline {
    /// Construit un nouveau pipeline en status `queued`.
    pub fn new(
        repository_id: Uuid,
        commit_id: String,
        trigger_event: TriggerEvent,
        pipeline_name: Option<String>,
        creator_id: Option<Uuid>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repository_id,
            commit_id,
            trigger_event,
            status: PipelineStatus::Queued,
            pipeline_name,
            started_at: None,
            finished_at: None,
            duration_ms: None,
            creator_id,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── PipelineStage ───────────────────────────────────────────────────

/// Étape individuelle d'un pipeline CI/CD.
///
/// Chaque stage s'exécute dans un container Docker isolé.
/// Les logs stdout/stderr sont accumulés dans le champ `logs`.
#[derive(Debug, Clone)]
pub struct PipelineStage {
    /// Identifiant unique.
    pub id: Uuid,
    /// Pipeline parent.
    pub pipeline_id: Uuid,
    /// Nom du stage (ex: "Suiton_Build", "Katon_Test").
    pub name: String,
    /// Image Docker OCI (ex: "rust:1.80-slim").
    pub image: String,
    /// Statut du stage.
    pub status: PipelineStageStatus,
    /// Ordre d'exécution (0-based).
    pub sort_order: i16,
    /// Début d'exécution.
    pub started_at: Option<DateTime<Utc>>,
    /// Fin d'exécution.
    pub finished_at: Option<DateTime<Utc>>,
    /// Durée en millisecondes.
    pub duration_ms: Option<i32>,
    /// Logs stdout/stderr du container.
    pub logs: Option<String>,
    /// Code de sortie du container (0 = succès).
    pub exit_code: Option<i16>,
    /// Date de création en base.
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour.
    pub updated_at: DateTime<Utc>,
}

impl PipelineStage {
    /// Construit un nouveau stage en status `pending`.
    pub fn new(
        pipeline_id: Uuid,
        name: String,
        image: String,
        sort_order: i16,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            pipeline_id,
            name,
            image,
            status: PipelineStageStatus::Pending,
            sort_order,
            started_at: None,
            finished_at: None,
            duration_ms: None,
            logs: None,
            exit_code: None,
            created_at: now,
            updated_at: now,
        }
    }
}
