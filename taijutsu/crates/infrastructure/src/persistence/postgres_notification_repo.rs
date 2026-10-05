//! Adaptateur PostgreSQL — NotificationRepository (Phase 38 — Le Carillon) 🔔
//!
//! Implémente le port `NotificationRepository` pour le stockage PostgreSQL.
//! Utilise `ON CONFLICT DO NOTHING` pour la déduplication silencieuse.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use domain::entities::notification::{Notification, NotificationType, TargetType};
use domain::errors::DomainError;
use domain::ports::notification_repository::NotificationRepository;

/// Adaptateur PostgreSQL pour les notifications.
pub struct PostgresNotificationRepo {
    pool: PgPool,
}

impl PostgresNotificationRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotificationRepository for PostgresNotificationRepo {
    async fn save(&self, notification: &Notification) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO notifications (
                id, recipient_id, actor_id, notification_type, target_type,
                target_id, target_number, repository_id, repository_owner,
                repository_name, message, read, created_at
            ) VALUES ($1, $2, $3, $4::notification_type, $5::notification_target_type,
                      $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (recipient_id, actor_id, notification_type, target_id) DO NOTHING
            "#,
        )
        .bind(notification.id)
        .bind(notification.recipient_id)
        .bind(notification.actor_id)
        .bind(notification.notification_type.as_sql_str())
        .bind(notification.target_type.as_sql_str())
        .bind(notification.target_id)
        .bind(notification.target_number)
        .bind(notification.repository_id)
        .bind(&notification.repository_owner)
        .bind(&notification.repository_name)
        .bind(&notification.message)
        .bind(notification.read)
        .bind(notification.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("notification save: {e}")))?;

        Ok(())
    }

    async fn list_for_recipient(
        &self,
        recipient_id: &Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Notification>, i64), DomainError> {
        let rows = sqlx::query_as::<_, NotificationRow>(
            r#"
            SELECT id, recipient_id, actor_id,
                   notification_type::TEXT AS notification_type,
                   target_type::TEXT AS target_type,
                   target_id, target_number, repository_id,
                   repository_owner, repository_name,
                   message, read, read_at, created_at
            FROM notifications
            WHERE recipient_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(recipient_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("notification list: {e}")))?;

        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM notifications WHERE recipient_id = $1",
        )
        .bind(recipient_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("notification count: {e}")))?;

        let notifications = rows.into_iter().map(|r| r.into_domain()).collect();
        Ok((notifications, total.0))
    }

    async fn count_unread(&self, recipient_id: &Uuid) -> Result<i64, DomainError> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM notifications WHERE recipient_id = $1 AND read = FALSE",
        )
        .bind(recipient_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("notification unread count: {e}")))?;

        Ok(count)
    }

    async fn mark_read(&self, id: &Uuid, recipient_id: &Uuid) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE notifications
            SET read = TRUE, read_at = NOW()
            WHERE id = $1 AND recipient_id = $2 AND read = FALSE
            "#,
        )
        .bind(id)
        .bind(recipient_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("notification mark_read: {e}")))?;

        Ok(())
    }

    async fn mark_all_read(&self, recipient_id: &Uuid) -> Result<i64, DomainError> {
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET read = TRUE, read_at = NOW()
            WHERE recipient_id = $1 AND read = FALSE
            "#,
        )
        .bind(recipient_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("notification mark_all_read: {e}")))?;

        Ok(result.rows_affected() as i64)
    }
}

// ── Row mapper ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    recipient_id: Uuid,
    actor_id: Uuid,
    notification_type: String,
    target_type: String,
    target_id: Uuid,
    target_number: Option<i32>,
    repository_id: Uuid,
    repository_owner: String,
    repository_name: String,
    message: String,
    read: bool,
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl NotificationRow {
    fn into_domain(self) -> Notification {
        Notification {
            id: self.id,
            recipient_id: self.recipient_id,
            actor_id: self.actor_id,
            notification_type: NotificationType::from_sql_str(&self.notification_type)
                .unwrap_or(NotificationType::Mentioned),
            target_type: TargetType::from_sql_str(&self.target_type)
                .unwrap_or(TargetType::Issue),
            target_id: self.target_id,
            target_number: self.target_number,
            repository_id: self.repository_id,
            repository_owner: self.repository_owner,
            repository_name: self.repository_name,
            message: self.message,
            read: self.read,
            read_at: self.read_at,
            created_at: self.created_at,
        }
    }
}
