//! Port: NotificationRepository — Phase 38 (Le Carillon) 🔔
//!
//! Contrat d'accès aux notifications in-app.
//! L'adaptateur concret dans infrastructure/ implémente le stockage PostgreSQL.
//!
//! ## Déduplication
//! L'index `UNIQUE (recipient_id, actor_id, notification_type, target_id)`
//! empêche les doublons. L'adapter doit utiliser `ON CONFLICT DO NOTHING`
//! dans `save()` pour ignorer silencieusement les re-mentions.

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::notification::Notification;
use crate::errors::DomainError;

/// Contrat d'accès aux notifications in-app.
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Persiste une notification. Ignore silencieusement les doublons
    /// (index UNIQUE de déduplication).
    async fn save(&self, notification: &Notification) -> Result<(), DomainError>;

    /// Liste les notifications d'un destinataire (ordre décroissant par date).
    /// Retourne `(notifications, total_count)`.
    async fn list_for_recipient(
        &self,
        recipient_id: &Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Notification>, i64), DomainError>;

    /// Compte les notifications non-lues d'un destinataire.
    /// Utilise l'index partiel `WHERE read = FALSE` pour O(1).
    async fn count_unread(&self, recipient_id: &Uuid) -> Result<i64, DomainError>;

    /// Marque une notification comme lue.
    /// Vérifie que le `recipient_id` correspond (sécurité RBAC).
    async fn mark_read(&self, id: &Uuid, recipient_id: &Uuid) -> Result<(), DomainError>;

    /// Marque toutes les notifications d'un destinataire comme lues.
    /// Retourne le nombre de notifications mises à jour.
    async fn mark_all_read(&self, recipient_id: &Uuid) -> Result<i64, DomainError>;
}
