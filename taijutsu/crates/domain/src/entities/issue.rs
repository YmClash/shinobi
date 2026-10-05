//! Entité Issue — Système de suivi de tickets (Phase 33 — Le Parchemin des Doléances).
//!
//! Représente un ticket de suivi (bug, feature, tâche) au sein d'un dépôt.
//! Le compteur de numérotation est partagé avec les Merge Requests
//! (`repo_counters.next_ticket_number`) pour garantir l'unicité globale
//! de `#ID` au sein d'un dépôt (convention GitHub/GitLab).
//!
//! ## Cycle de vie
//! ```text
//! [Open] → Closed → Reopened → Closed → ...
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Issue ───────────────────────────────────────────────────────────────

/// Ticket de suivi — entité centrale du système d'issues.
#[derive(Debug, Clone)]
pub struct Issue {
    /// Identifiant unique.
    pub id: Uuid,
    /// Dépôt auquel l'issue appartient (isolation multi-tenant).
    pub repository_id: Uuid,
    /// Acteur ayant ouvert l'issue (humain ou bot IA).
    pub author_id: Uuid,
    /// Numéro séquentiel par repo (ex: #1, #2, #3...).
    /// Partagé avec les MR pour unicité globale.
    pub number: i32,
    /// Titre court de l'issue.
    pub title: String,
    /// Description détaillée (Markdown).
    pub body: Option<String>,
    /// État actuel de l'issue.
    pub status: IssueStatus,
    /// Acteur assigné à l'issue (`None` si non assignée).
    pub assignee_id: Option<Uuid>,
    /// Acteur ayant fermé l'issue (`None` si pas encore fermée).
    pub closed_by: Option<Uuid>,
    /// Timestamp de la fermeture (`None` si pas encore fermée).
    pub closed_at: Option<DateTime<Utc>>,
    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de dernière mise à jour.
    pub updated_at: DateTime<Utc>,
}

impl Issue {
    /// Construit une nouvelle issue ouverte.
    pub fn new(
        repository_id: Uuid,
        author_id: Uuid,
        number: i32,
        title: String,
        body: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repository_id,
            author_id,
            number,
            title,
            body,
            status: IssueStatus::Open,
            assignee_id: None,
            closed_by: None,
            closed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// L'issue est-elle ouverte ?
    pub fn is_open(&self) -> bool {
        self.status == IssueStatus::Open
    }

    /// L'issue est-elle fermée ?
    pub fn is_closed(&self) -> bool {
        self.status == IssueStatus::Closed
    }
}

// ── IssueStatus ─────────────────────────────────────────────────────────

/// État du cycle de vie d'une Issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueStatus {
    /// Ouverte — en cours de résolution.
    Open,
    /// Fermée — résolue ou abandonnée.
    Closed,
}

impl IssueStatus {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

// ── IssueComment ────────────────────────────────────────────────────────

/// Commentaire sur une issue — corps Markdown.
#[derive(Debug, Clone)]
pub struct IssueComment {
    /// Identifiant unique du commentaire.
    pub id: Uuid,
    /// Issue commentée.
    pub issue_id: Uuid,
    /// Acteur ayant posté le commentaire.
    pub author_id: Uuid,
    /// Corps du commentaire (Markdown).
    pub body: String,
    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de dernière modification.
    pub updated_at: DateTime<Utc>,
}

impl IssueComment {
    /// Construit un nouveau commentaire.
    pub fn new(
        issue_id: Uuid,
        author_id: Uuid,
        body: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            issue_id,
            author_id,
            body,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── IssueEvent ──────────────────────────────────────────────────────────

/// Événement dans la timeline d'une issue (audit trail).
#[derive(Debug, Clone)]
pub struct IssueEvent {
    /// Identifiant unique.
    pub id: Uuid,
    /// Issue concernée.
    pub issue_id: Uuid,
    /// Acteur ayant déclenché l'événement.
    pub actor_id: Uuid,
    /// Type d'événement.
    pub event_type: IssueEventType,
    /// Payload JSON libre (ex: ancien titre, `closed_by_mr`, etc.).
    pub payload: serde_json::Value,
    /// Date de l'événement.
    pub created_at: DateTime<Utc>,
}

impl IssueEvent {
    /// Construit un événement de timeline.
    pub fn new(
        issue_id: Uuid,
        actor_id: Uuid,
        event_type: IssueEventType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            issue_id,
            actor_id,
            event_type,
            payload,
            created_at: Utc::now(),
        }
    }
}

// ── IssueEventType ──────────────────────────────────────────────────────

/// Types d'événements dans la timeline d'une issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueEventType {
    /// Issue ouverte.
    Opened,
    /// Issue fermée.
    Closed,
    /// Issue rouverte après fermeture.
    Reopened,
    /// Commentaire ajouté.
    Commented,
    /// Titre modifié.
    TitleChanged,
    /// Corps (description) modifié.
    BodyChanged,
    /// Label ajouté.
    LabelAdded,
    /// Label retiré.
    LabelRemoved,
    /// Assigné à un acteur.
    Assigned,
    /// Désassigné d'un acteur.
    Unassigned,
    /// Un acteur a été mentionné via @handle (Phase 37C).
    Mentioned,
}

impl IssueEventType {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
            Self::Commented => "commented",
            Self::TitleChanged => "title_changed",
            Self::BodyChanged => "body_changed",
            Self::LabelAdded => "label_added",
            Self::LabelRemoved => "label_removed",
            Self::Assigned => "assigned",
            Self::Unassigned => "unassigned",
            Self::Mentioned => "mentioned",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "opened" => Some(Self::Opened),
            "closed" => Some(Self::Closed),
            "reopened" => Some(Self::Reopened),
            "commented" => Some(Self::Commented),
            "title_changed" => Some(Self::TitleChanged),
            "body_changed" => Some(Self::BodyChanged),
            "label_added" => Some(Self::LabelAdded),
            "label_removed" => Some(Self::LabelRemoved),
            "assigned" => Some(Self::Assigned),
            "unassigned" => Some(Self::Unassigned),
            "mentioned" => Some(Self::Mentioned),
            _ => None,
        }
    }
}

// ── IssueLabel ──────────────────────────────────────────────────────────

/// Label (étiquette colorée) — scopé par repository.
#[derive(Debug, Clone, Serialize)]
pub struct IssueLabel {
    /// Identifiant unique.
    pub id: Uuid,
    /// Dépôt propriétaire du label.
    pub repository_id: Uuid,
    /// Nom du label (ex: "bug", "feature", "P1").
    pub name: String,
    /// Couleur hex (ex: "#d73a4a" pour rouge bug).
    pub color: String,
    /// Description optionnelle.
    pub description: Option<String>,
}

impl IssueLabel {
    /// Construit un nouveau label.
    pub fn new(
        repository_id: Uuid,
        name: String,
        color: String,
        description: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            repository_id,
            name,
            color,
            description,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_issue_has_open_status() {
        let issue = Issue::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            1,
            "Fix login bug".to_string(),
            Some("Description".to_string()),
        );
        assert!(issue.is_open());
        assert!(!issue.is_closed());
        assert_eq!(issue.number, 1);
        assert!(issue.assignee_id.is_none());
        assert!(issue.closed_by.is_none());
    }

    #[test]
    fn test_issue_status_roundtrip() {
        for status in [IssueStatus::Open, IssueStatus::Closed] {
            let sql = status.as_sql_str();
            let parsed = IssueStatus::from_sql_str(sql).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_issue_event_type_roundtrip() {
        let types = [
            IssueEventType::Opened,
            IssueEventType::Closed,
            IssueEventType::Reopened,
            IssueEventType::Commented,
            IssueEventType::TitleChanged,
            IssueEventType::BodyChanged,
            IssueEventType::LabelAdded,
            IssueEventType::LabelRemoved,
            IssueEventType::Assigned,
            IssueEventType::Unassigned,
            IssueEventType::Mentioned,
        ];
        for t in types {
            let sql = t.as_sql_str();
            let parsed = IssueEventType::from_sql_str(sql).unwrap();
            assert_eq!(t, parsed);
        }
    }

    #[test]
    fn test_issue_comment_new() {
        let comment = IssueComment::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "This is a comment".to_string(),
        );
        assert_eq!(comment.body, "This is a comment");
    }

    #[test]
    fn test_issue_event_new() {
        let event = IssueEvent::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            IssueEventType::Opened,
            serde_json::json!({"title": "Initial"}),
        );
        assert_eq!(event.event_type, IssueEventType::Opened);
    }

    #[test]
    fn test_issue_label_new() {
        let label = IssueLabel::new(
            Uuid::new_v4(),
            "bug".to_string(),
            "#d73a4a".to_string(),
            Some("Something isn't working".to_string()),
        );
        assert_eq!(label.name, "bug");
        assert_eq!(label.color, "#d73a4a");
    }

    #[test]
    fn test_issue_status_serde_json() {
        let json = serde_json::to_string(&IssueStatus::Open).unwrap();
        assert_eq!(json, "\"open\"");
        let deserialized: IssueStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, IssueStatus::Open);
    }

    #[test]
    fn test_issue_event_type_serde_json() {
        let json = serde_json::to_string(&IssueEventType::LabelAdded).unwrap();
        assert_eq!(json, "\"label_added\"");
        let deserialized: IssueEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, IssueEventType::LabelAdded);
    }
}
