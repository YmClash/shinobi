//! Adaptateur Fūinjutsu — PostgreSQL OperationRepository.
//!
//! Implémentation concrète du port `OperationRepository`
//! utilisant PostgreSQL via sqlx pour la persistence asynchrone.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;

/// Adaptateur PostgreSQL pour la persistence des opérations.
///
/// Encapsule un pool de connexions `sqlx::PgPool`.
#[derive(Debug, Clone)]
pub struct PostgresOperationRepository {
    pool: PgPool,
}

impl PostgresOperationRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit une entité `Operation` à partir d'une ligne PostgreSQL.
fn row_to_operation(row: sqlx::postgres::PgRow) -> Result<Operation, DomainError> {
    let parent_ids_json: serde_json::Value = row
        .try_get("parent_ids")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

    let parent_ids: Vec<Uuid> = serde_json::from_value(parent_ids_json)
        .map_err(|e| DomainError::Persistence(format!("parent_ids JSON invalide: {e}")))?;

    Ok(Operation {
        id: row
            .try_get("id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        author_id: row
            .try_get("author_id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        content_id: ContentId::new(
            row.try_get::<String, _>("content_id")
                .map_err(|e| DomainError::Persistence(e.to_string()))?,
        ),
        description: row
            .try_get("description")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        parent_ids,
        created_at: row
            .try_get::<DateTime<Utc>, _>("created_at")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

#[async_trait]
impl OperationRepository for PostgresOperationRepository {
    #[instrument(skip(self, operation), fields(operation_id = %operation.id))]
    async fn save(&self, operation: &Operation) -> Result<(), DomainError> {
        let parent_ids_json = serde_json::to_value(&operation.parent_ids)
            .map_err(|e| DomainError::Persistence(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO operations (id, author_id, content_id, description, parent_ids, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(operation.id)
        .bind(operation.author_id)
        .bind(operation.content_id.as_str())
        .bind(&operation.description)
        .bind(&parent_ids_json)
        .bind(operation.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Operation>, DomainError> {
        let row = sqlx::query(
            "SELECT id, author_id, content_id, description, parent_ids, created_at \
             FROM operations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_operation(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn list_recent(&self, limit: usize) -> Result<Vec<Operation>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, author_id, content_id, description, parent_ids, created_at \
             FROM operations ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_operation).collect()
    }

    #[instrument(skip(self))]
    async fn find_by_author(&self, author_id: &Uuid) -> Result<Vec<Operation>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, author_id, content_id, description, parent_ids, created_at \
             FROM operations WHERE author_id = $1 ORDER BY created_at DESC",
        )
        .bind(author_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_operation).collect()
    }
}
