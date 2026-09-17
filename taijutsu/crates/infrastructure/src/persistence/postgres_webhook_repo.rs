//! Adaptateur PostgreSQL — WebhookRepository (Phase 34 — Chakra チャクラ)
//!
//! Implémente le port `WebhookRepository` pour le stockage PostgreSQL.
//! Gère les webhooks et leur historique de livraison.
//!
//! ## Pattern
//! - `events` stocké en `webhook_event_type[]` (array PostgreSQL natif)
//! - Cast `::TEXT[]` pour la lecture, `::webhook_event_type[]` pour l'écriture
//! - Row mappers privés pour isoler la couche SQL de la couche domaine

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use domain::entities::webhook::{Webhook, WebhookDelivery, WebhookEventType};
use domain::errors::DomainError;
use domain::ports::webhook_repository::WebhookRepository;

/// Adaptateur PostgreSQL pour les webhooks.
pub struct PostgresWebhookRepo {
    pool: PgPool,
}

impl PostgresWebhookRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebhookRepository for PostgresWebhookRepo {
    async fn save(&self, webhook: &Webhook) -> Result<(), DomainError> {
        let events_str: Vec<&str> = webhook.events.iter().map(|e| e.as_sql_str()).collect();

        sqlx::query(
            r#"
            INSERT INTO webhooks (
                id, repository_id, creator_id, url, secret, events,
                active, failure_count, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6::webhook_event_type[],
                $7, $8, $9, $10
            )
            "#,
        )
        .bind(webhook.id)
        .bind(webhook.repository_id)
        .bind(webhook.creator_id)
        .bind(&webhook.url)
        .bind(&webhook.secret)
        .bind(&events_str)
        .bind(webhook.active)
        .bind(webhook.failure_count)
        .bind(webhook.created_at)
        .bind(webhook.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook save: {e}")))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Webhook>, DomainError> {
        let row = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT id, repository_id, creator_id, url, secret,
                   events::TEXT[] AS events,
                   active, last_delivery_at, failure_count,
                   created_at, updated_at
            FROM webhooks
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook find_by_id: {e}")))?;

        Ok(row.map(|r| r.into_domain()))
    }

    async fn list_by_repository(&self, repository_id: &Uuid) -> Result<Vec<Webhook>, DomainError> {
        let rows = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT id, repository_id, creator_id, url, secret,
                   events::TEXT[] AS events,
                   active, last_delivery_at, failure_count,
                   created_at, updated_at
            FROM webhooks
            WHERE repository_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(repository_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook list_by_repository: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    async fn find_active_for_event(
        &self,
        repository_id: &Uuid,
        event_type: &str,
    ) -> Result<Vec<Webhook>, DomainError> {
        let rows = sqlx::query_as::<_, WebhookRow>(
            r#"
            SELECT id, repository_id, creator_id, url, secret,
                   events::TEXT[] AS events,
                   active, last_delivery_at, failure_count,
                   created_at, updated_at
            FROM webhooks
            WHERE repository_id = $1
              AND active = TRUE
              AND $2::webhook_event_type = ANY(events)
            "#,
        )
        .bind(repository_id)
        .bind(event_type)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook find_active_for_event: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    async fn update(&self, webhook: &Webhook) -> Result<(), DomainError> {
        let events_str: Vec<&str> = webhook.events.iter().map(|e| e.as_sql_str()).collect();

        sqlx::query(
            r#"
            UPDATE webhooks
            SET url = $2,
                events = $3::webhook_event_type[],
                active = $4,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(webhook.id)
        .bind(&webhook.url)
        .bind(&events_str)
        .bind(webhook.active)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook update: {e}")))?;

        Ok(())
    }

    async fn delete(&self, id: &Uuid) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM webhooks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Persistence(format!("webhook delete: {e}")))?;

        Ok(())
    }

    async fn count_by_repository(&self, repository_id: &Uuid) -> Result<i64, DomainError> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM webhooks WHERE repository_id = $1",
        )
        .bind(repository_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook count: {e}")))?;

        Ok(count)
    }

    async fn update_failure_count(
        &self,
        webhook_id: &Uuid,
        reset: bool,
    ) -> Result<(), DomainError> {
        if reset {
            sqlx::query(
                "UPDATE webhooks SET failure_count = 0, updated_at = NOW() WHERE id = $1",
            )
            .bind(webhook_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Persistence(format!("webhook reset failure: {e}")))?;
        } else {
            sqlx::query(
                "UPDATE webhooks SET failure_count = failure_count + 1, updated_at = NOW() WHERE id = $1",
            )
            .bind(webhook_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Persistence(format!("webhook inc failure: {e}")))?;
        }

        Ok(())
    }

    async fn update_last_delivery(
        &self,
        webhook_id: &Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE webhooks SET last_delivery_at = NOW(), updated_at = NOW() WHERE id = $1",
        )
        .bind(webhook_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook update_last_delivery: {e}")))?;

        Ok(())
    }

    // ── Deliveries ────────────────────────────────────────────────

    async fn save_delivery(&self, delivery: &WebhookDelivery) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO webhook_deliveries (
                id, webhook_id, event_type, event_id, url,
                request_headers, request_body,
                response_status, response_body, response_headers,
                duration_ms, success, attempt, error_message, created_at
            ) VALUES (
                $1, $2, $3::webhook_event_type, $4, $5,
                $6, $7,
                $8, $9, $10,
                $11, $12, $13, $14, $15
            )
            "#,
        )
        .bind(delivery.id)
        .bind(delivery.webhook_id)
        .bind(delivery.event_type.as_sql_str())
        .bind(delivery.event_id)
        .bind(&delivery.url)
        .bind(&delivery.request_headers)
        .bind(&delivery.request_body)
        .bind(delivery.response_status)
        .bind(&delivery.response_body)
        .bind(&delivery.response_headers)
        .bind(delivery.duration_ms.map(|d| d as i32))
        .bind(delivery.success)
        .bind(delivery.attempt)
        .bind(&delivery.error_message)
        .bind(delivery.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook_delivery save: {e}")))?;

        Ok(())
    }

    async fn list_deliveries(
        &self,
        webhook_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DomainError> {
        let rows = sqlx::query_as::<_, DeliveryRow>(
            r#"
            SELECT id, webhook_id, event_type::TEXT AS event_type, event_id, url,
                   request_headers, request_body,
                   response_status, response_body, response_headers,
                   duration_ms, success, attempt, error_message, created_at
            FROM webhook_deliveries
            WHERE webhook_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(webhook_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook_delivery list: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    async fn find_delivery_by_id(
        &self,
        delivery_id: &Uuid,
    ) -> Result<Option<WebhookDelivery>, DomainError> {
        let row = sqlx::query_as::<_, DeliveryRow>(
            r#"
            SELECT id, webhook_id, event_type::TEXT AS event_type, event_id, url,
                   request_headers, request_body,
                   response_status, response_body, response_headers,
                   duration_ms, success, attempt, error_message, created_at
            FROM webhook_deliveries
            WHERE id = $1
            "#,
        )
        .bind(delivery_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("webhook_delivery find: {e}")))?;

        Ok(row.map(|r| r.into_domain()))
    }
}

// ── Row Mappers ───────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct WebhookRow {
    id: Uuid,
    repository_id: Uuid,
    creator_id: Uuid,
    url: String,
    secret: String,
    events: Vec<String>,
    active: bool,
    last_delivery_at: Option<chrono::DateTime<chrono::Utc>>,
    failure_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl WebhookRow {
    fn into_domain(self) -> Webhook {
        Webhook {
            id: self.id,
            repository_id: self.repository_id,
            creator_id: self.creator_id,
            url: self.url,
            secret: self.secret,
            events: self
                .events
                .iter()
                .filter_map(|e| WebhookEventType::from_sql_str(e))
                .collect(),
            active: self.active,
            last_delivery_at: self.last_delivery_at,
            failure_count: self.failure_count,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DeliveryRow {
    id: Uuid,
    webhook_id: Uuid,
    event_type: String,
    event_id: Uuid,
    url: String,
    request_headers: serde_json::Value,
    request_body: String,
    response_status: Option<i16>,
    response_body: Option<String>,
    response_headers: Option<serde_json::Value>,
    duration_ms: Option<i32>,
    success: bool,
    attempt: i16,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl DeliveryRow {
    fn into_domain(self) -> WebhookDelivery {
        WebhookDelivery {
            id: self.id,
            webhook_id: self.webhook_id,
            event_type: WebhookEventType::from_sql_str(&self.event_type)
                .unwrap_or(WebhookEventType::Push),
            event_id: self.event_id,
            url: self.url,
            request_headers: self.request_headers,
            request_body: self.request_body,
            response_status: self.response_status,
            response_body: self.response_body,
            response_headers: self.response_headers,
            duration_ms: self.duration_ms.map(|d| d as i64),
            success: self.success,
            attempt: self.attempt,
            error_message: self.error_message,
            created_at: self.created_at,
        }
    }
}
