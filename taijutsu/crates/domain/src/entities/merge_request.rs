//! Entité MergeRequest — Modèle de collaboration (Phase 26A — Le Katana Croisé).
//!
//! Représente une demande de fusion entre une branche source et une branche cible.
//! Conçu pour être ForgeFed-compatible (Phase 27 — fédération inter-forges).
//!
//! ## Cycle de vie
//! ```text
//! [Draft] → Open → { Reviewed → Approved } → Merged
//!                 → Closed → Reopened → ...
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── MergeRequest ────────────────────────────────────────────────────────

/// Demande de fusion — entité centrale du système de collaboration.
#[derive(Debug, Clone)]
pub struct MergeRequest {
    /// Identifiant unique.
    pub id: Uuid,
    /// Dépôt auquel la MR appartient (isolation multi-tenant).
    pub repository_id: Uuid,
    /// Acteur ayant ouvert la MR (humain ou bot IA).
    pub author_id: Uuid,
    /// Numéro séquentiel par repo (ex: #1, #2, #3...).
    pub number: i32,
    /// Titre court de la MR.
    pub title: String,
    /// Description détaillée (Markdown).
    pub description: Option<String>,
    /// Nom de la branche source (ex: `feature/auth`).
    pub source_branch: String,
    /// Nom de la branche cible (ex: `main`).
    pub target_branch: String,
    /// État actuel de la MR.
    pub status: MrStatus,
    /// Acteur ayant fusionné la MR (`None` si pas encore mergée).
    pub merged_by: Option<Uuid>,
    /// Timestamp du merge (`None` si pas encore mergée).
    pub merged_at: Option<DateTime<Utc>>,
    /// Timestamp de la fermeture sans merge (`None` si pas fermée).
    pub closed_at: Option<DateTime<Utc>>,
    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour.
    pub updated_at: DateTime<Utc>,
}

impl MergeRequest {
    /// Construit une nouvelle MR ouverte.
    pub fn new(
        repository_id: Uuid,
        author_id: Uuid,
        number: i32,
        title: String,
        description: Option<String>,
        source_branch: String,
        target_branch: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repository_id,
            author_id,
            number,
            title,
            description,
            source_branch,
            target_branch,
            status: MrStatus::Open,
            merged_by: None,
            merged_at: None,
            closed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// La MR est-elle ouverte ?
    pub fn is_open(&self) -> bool {
        self.status == MrStatus::Open
    }

    /// La MR a-t-elle été fusionnée ?
    pub fn is_merged(&self) -> bool {
        self.status == MrStatus::Merged
    }

    /// La MR est-elle fermée sans merge ?
    pub fn is_closed(&self) -> bool {
        self.status == MrStatus::Closed
    }
}

// ── MrStatus ────────────────────────────────────────────────────────────

/// État du cycle de vie d'une Merge Request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MrStatus {
    /// Ouverte — en attente de review/merge.
    Open,
    /// Fusionnée — les changements sont intégrés dans la branche cible.
    Merged,
    /// Fermée — sans fusion (annulée ou rejetée).
    Closed,
}

impl MrStatus {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "merged" => Some(Self::Merged),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

// ── MrReview ────────────────────────────────────────────────────────────

/// Review globale d'une MR — verdict + commentaire Markdown.
#[derive(Debug, Clone)]
pub struct MrReview {
    /// Identifiant unique de la review.
    pub id: Uuid,
    /// MR reviewée.
    pub mr_id: Uuid,
    /// Acteur ayant soumis la review (humain ou Oracle bot).
    pub reviewer_id: Uuid,
    /// Verdict : approuver ou demander des changements.
    pub verdict: MrVerdict,
    /// Corps de la review (Markdown).
    pub body: Option<String>,
    /// Date de création.
    pub created_at: DateTime<Utc>,
}

impl MrReview {
    /// Construit une nouvelle review.
    pub fn new(
        mr_id: Uuid,
        reviewer_id: Uuid,
        verdict: MrVerdict,
        body: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            mr_id,
            reviewer_id,
            verdict,
            body,
            created_at: Utc::now(),
        }
    }
}

// ── MrVerdict ───────────────────────────────────────────────────────────

/// Verdict d'une review de MR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MrVerdict {
    /// Approuvé — la MR est prête à être fusionnée.
    Approve,
    /// Changements demandés — l'auteur doit corriger avant fusion.
    ChangesRequested,
}

impl MrVerdict {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::ChangesRequested => "changes_requested",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "approve" => Some(Self::Approve),
            "changes_requested" => Some(Self::ChangesRequested),
            _ => None,
        }
    }
}

// ── MrEvent ─────────────────────────────────────────────────────────────

/// Événement dans la timeline d'une MR (audit trail).
#[derive(Debug, Clone)]
pub struct MrEvent {
    /// Identifiant unique.
    pub id: Uuid,
    /// MR concernée.
    pub mr_id: Uuid,
    /// Acteur ayant déclenché l'événement.
    pub actor_id: Uuid,
    /// Type d'événement.
    pub event_type: MrEventType,
    /// Payload JSON libre (ex: ancien titre, hash du commit, etc.).
    pub payload: serde_json::Value,
    /// Date de l'événement.
    pub created_at: DateTime<Utc>,
}

impl MrEvent {
    /// Construit un événement de timeline.
    pub fn new(
        mr_id: Uuid,
        actor_id: Uuid,
        event_type: MrEventType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            mr_id,
            actor_id,
            event_type,
            payload,
            created_at: Utc::now(),
        }
    }
}

// ── MrEventType ─────────────────────────────────────────────────────────

/// Types d'événements dans la timeline d'une MR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MrEventType {
    /// MR ouverte.
    Opened,
    /// Nouveaux commits poussés sur la branche source.
    Pushed,
    /// Review soumise (verdict inclus dans le payload).
    Reviewed,
    /// Review approuvée (shortcut pour filtrage rapide).
    Approved,
    /// Changements demandés.
    ChangesRequested,
    /// MR fusionnée.
    Merged,
    /// MR fermée sans fusion.
    Closed,
    /// MR rouverte après fermeture.
    Reopened,
    /// Un acteur a été mentionné via @handle (Phase 37C).
    Mentioned,
}

impl MrEventType {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Pushed => "pushed",
            Self::Reviewed => "reviewed",
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
            Self::Mentioned => "mentioned",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "opened" => Some(Self::Opened),
            "pushed" => Some(Self::Pushed),
            "reviewed" => Some(Self::Reviewed),
            "approved" => Some(Self::Approved),
            "changes_requested" => Some(Self::ChangesRequested),
            "merged" => Some(Self::Merged),
            "closed" => Some(Self::Closed),
            "reopened" => Some(Self::Reopened),
            "mentioned" => Some(Self::Mentioned),
            _ => None,
        }
    }
}

// ── MergeStrategy ───────────────────────────────────────────────────────

/// Stratégie de fusion pour une MR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Fast-forward : avance le bookmark target vers le commit source.
    /// Possible uniquement si target est un ancêtre de source.
    FastForward,
    /// Squash : compresse tous les commits de la branche source en un seul.
    Squash,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_mr_has_open_status() {
        let mr = MergeRequest::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            1,
            "Add feature X".to_string(),
            Some("Description".to_string()),
            "feature/x".to_string(),
            "main".to_string(),
        );
        assert!(mr.is_open());
        assert!(!mr.is_merged());
        assert!(!mr.is_closed());
        assert_eq!(mr.number, 1);
        assert!(mr.merged_by.is_none());
    }

    #[test]
    fn test_mr_status_roundtrip() {
        for status in [MrStatus::Open, MrStatus::Merged, MrStatus::Closed] {
            let sql = status.as_sql_str();
            let parsed = MrStatus::from_sql_str(sql).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_mr_verdict_roundtrip() {
        for verdict in [MrVerdict::Approve, MrVerdict::ChangesRequested] {
            let sql = verdict.as_sql_str();
            let parsed = MrVerdict::from_sql_str(sql).unwrap();
            assert_eq!(verdict, parsed);
        }
    }

    #[test]
    fn test_mr_event_type_roundtrip() {
        let types = [
            MrEventType::Opened,
            MrEventType::Pushed,
            MrEventType::Reviewed,
            MrEventType::Approved,
            MrEventType::ChangesRequested,
            MrEventType::Merged,
            MrEventType::Closed,
            MrEventType::Reopened,
            MrEventType::Mentioned,
        ];
        for t in types {
            let sql = t.as_sql_str();
            let parsed = MrEventType::from_sql_str(sql).unwrap();
            assert_eq!(t, parsed);
        }
    }

    #[test]
    fn test_mr_review_new() {
        let review = MrReview::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MrVerdict::Approve,
            Some("LGTM!".to_string()),
        );
        assert_eq!(review.verdict, MrVerdict::Approve);
        assert_eq!(review.body.as_deref(), Some("LGTM!"));
    }

    #[test]
    fn test_mr_event_new() {
        let event = MrEvent::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MrEventType::Opened,
            serde_json::json!({"title": "Initial"}),
        );
        assert_eq!(event.event_type, MrEventType::Opened);
    }

    #[test]
    fn test_mr_status_serde_json() {
        let json = serde_json::to_string(&MrStatus::Open).unwrap();
        assert_eq!(json, "\"open\"");
        let deserialized: MrStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, MrStatus::Open);
    }

    #[test]
    fn test_merge_strategy_serde_json() {
        let json = serde_json::to_string(&MergeStrategy::FastForward).unwrap();
        assert_eq!(json, "\"fast_forward\"");
        let deserialized: MergeStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, MergeStrategy::FastForward);
    }
}
