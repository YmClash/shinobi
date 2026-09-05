//! Entité Notification — Phase 38 (Le Carillon) 🔔
//!
//! Représente une notification in-app ciblée vers un acteur.
//! Produite automatiquement par les événements système (@mentions,
//! reviews, assignations) et consommée par le frontend (bell + dropdown).
//!
//! ## Design decisions
//! - **Dénormalisé** : `repository_owner` + `repository_name` stockés
//!   directement pour éviter les N+1 queries lors de l'affichage.
//! - **Dédup** : Index unique `(recipient, actor, type, target_id)` empêche
//!   le spam (une seule notification par mention par ticket).
//! - **Polymorphique** : `target_id` est un UUID libre (peut pointer vers
//!   une Issue, MR, ou commentaire) — pas de FK contrainte.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Notification ───────────────────────────────────────────────────────

/// Notification in-app ciblée vers un acteur.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Identifiant unique.
    pub id: Uuid,
    /// Acteur destinataire de la notification.
    pub recipient_id: Uuid,
    /// Acteur ayant déclenché l'action.
    pub actor_id: Uuid,
    /// Type de notification.
    pub notification_type: NotificationType,
    /// Type de la cible (issue, MR, commentaire).
    pub target_type: TargetType,
    /// UUID de la cible (polymorphique — pas de FK contrainte).
    pub target_id: Uuid,
    /// Numéro séquentiel de la cible (#N) pour navigation frontend.
    pub target_number: Option<i32>,
    /// Repository ID (pour construire l'URL).
    pub repository_id: Uuid,
    /// Handle du propriétaire du repo (dénormalisé pour éviter N+1).
    pub repository_owner: String,
    /// Nom du repo (dénormalisé).
    pub repository_name: String,
    /// Message humain (ex: "naruto mentioned you in Issue #1").
    pub message: String,
    /// La notification a-t-elle été lue ?
    pub read: bool,
    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de lecture (si lue).
    pub read_at: Option<DateTime<Utc>>,
}

impl Notification {
    /// Construit une nouvelle notification non-lue.
    pub fn new(
        recipient_id: Uuid,
        actor_id: Uuid,
        notification_type: NotificationType,
        target_type: TargetType,
        target_id: Uuid,
        target_number: Option<i32>,
        repository_id: Uuid,
        repository_owner: String,
        repository_name: String,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            recipient_id,
            actor_id,
            notification_type,
            target_type,
            target_id,
            target_number,
            repository_id,
            repository_owner,
            repository_name,
            message,
            read: false,
            created_at: Utc::now(),
            read_at: None,
        }
    }
}

// ── NotificationType ───────────────────────────────────────────────────

/// Types de notifications — extensible pour les phases futures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    /// @mention dans issue/MR/commentaire (Phase 38 V1).
    Mentioned,
    /// Demande de review MR (future).
    ReviewRequested,
    /// Review reçue sur votre MR (future).
    ReviewReceived,
    /// Assigné à une issue (future).
    Assigned,
    /// Issue fermée (future).
    IssueClosed,
    /// MR fusionnée (future).
    MrMerged,
}

impl NotificationType {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Mentioned => "mentioned",
            Self::ReviewRequested => "review_requested",
            Self::ReviewReceived => "review_received",
            Self::Assigned => "assigned",
            Self::IssueClosed => "issue_closed",
            Self::MrMerged => "mr_merged",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "mentioned" => Some(Self::Mentioned),
            "review_requested" => Some(Self::ReviewRequested),
            "review_received" => Some(Self::ReviewReceived),
            "assigned" => Some(Self::Assigned),
            "issue_closed" => Some(Self::IssueClosed),
            "mr_merged" => Some(Self::MrMerged),
            _ => None,
        }
    }
}

// ── TargetType ─────────────────────────────────────────────────────────

/// Type de la cible d'une notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    /// Issue (ticket).
    Issue,
    /// Merge Request.
    MergeRequest,
    /// Commentaire.
    Comment,
}

impl TargetType {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::MergeRequest => "merge_request",
            Self::Comment => "comment",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "issue" => Some(Self::Issue),
            "merge_request" => Some(Self::MergeRequest),
            "comment" => Some(Self::Comment),
            _ => None,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_notification_is_unread() {
        let notif = Notification::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            NotificationType::Mentioned,
            TargetType::Issue,
            Uuid::new_v4(),
            Some(1),
            Uuid::new_v4(),
            "naruto".to_string(),
            "le-wm".to_string(),
            "naruto mentioned you in Issue #1".to_string(),
        );
        assert!(!notif.read);
        assert!(notif.read_at.is_none());
        assert_eq!(notif.notification_type, NotificationType::Mentioned);
        assert_eq!(notif.target_type, TargetType::Issue);
        assert_eq!(notif.repository_owner, "naruto");
        assert_eq!(notif.repository_name, "le-wm");
    }

    #[test]
    fn test_notification_type_roundtrip() {
        let types = [
            NotificationType::Mentioned,
            NotificationType::ReviewRequested,
            NotificationType::ReviewReceived,
            NotificationType::Assigned,
            NotificationType::IssueClosed,
            NotificationType::MrMerged,
        ];
        for t in types {
            let sql = t.as_sql_str();
            let parsed = NotificationType::from_sql_str(sql).unwrap();
            assert_eq!(t, parsed);
        }
    }

    #[test]
    fn test_target_type_roundtrip() {
        let types = [TargetType::Issue, TargetType::MergeRequest, TargetType::Comment];
        for t in types {
            let sql = t.as_sql_str();
            let parsed = TargetType::from_sql_str(sql).unwrap();
            assert_eq!(t, parsed);
        }
    }

    #[test]
    fn test_notification_type_serde_json() {
        let json = serde_json::to_string(&NotificationType::Mentioned).unwrap();
        assert_eq!(json, "\"mentioned\"");
        let deserialized: NotificationType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, NotificationType::Mentioned);
    }

    #[test]
    fn test_target_type_serde_json() {
        let json = serde_json::to_string(&TargetType::MergeRequest).unwrap();
        assert_eq!(json, "\"merge_request\"");
        let deserialized: TargetType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TargetType::MergeRequest);
    }
}
