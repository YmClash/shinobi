//! Adaptateur Fūinjutsu — Persistence des code reviews dans PostgreSQL.
//!
//! Implémente le port `ReviewRepository` via sqlx pour stocker les
//! reviews produites par l'agent Oracle dans la table `operation_reviews`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::review_repository::{OperationReview, ReviewRepository};

/// Repository PostgreSQL pour les code reviews de l'Oracle.
pub struct PostgresReviewRepository {
    pool: PgPool,
}

impl PostgresReviewRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReviewRepository for PostgresReviewRepository {
    #[instrument(skip(self, review), fields(operation_id = %review.operation_id, reviewer = %review.reviewer))]
    async fn save_review(&self, review: &OperationReview) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO operation_reviews (id, operation_id, reviewer, model, summary, content, score, duration_ms, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(review.id)
        .bind(review.operation_id)
        .bind(&review.reviewer)
        .bind(&review.model)
        .bind(&review.summary)
        .bind(&review.content)
        .bind(review.score)
        .bind(review.duration_ms as i64)
        .bind(review.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("save_review failed: {e}")))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_operation(
        &self,
        operation_id: &Uuid,
    ) -> Result<Vec<OperationReview>, DomainError> {
        let rows = sqlx::query_as::<_, ReviewRow>(
            r#"
            SELECT id, operation_id, reviewer, model, summary, content, score, duration_ms, created_at
            FROM operation_reviews
            WHERE operation_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(operation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("find_by_operation reviews failed: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    #[instrument(skip(self))]
    async fn delete_by_operation(&self, operation_id: &Uuid) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM operation_reviews WHERE operation_id = $1")
            .bind(operation_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DomainError::Persistence(format!("delete_by_operation reviews failed: {e}"))
            })?;

        Ok(result.rows_affected())
    }
}

// ── Row mapping (sqlx) ───────────────────────────────────────────────

/// Ligne de la table operation_reviews pour la désérialisation sqlx.
#[derive(sqlx::FromRow)]
struct ReviewRow {
    id: Uuid,
    operation_id: Uuid,
    reviewer: String,
    model: String,
    summary: String,
    content: String,
    score: Option<f32>,
    duration_ms: i64,
    created_at: DateTime<Utc>,
}

impl From<ReviewRow> for OperationReview {
    fn from(row: ReviewRow) -> Self {
        Self {
            id: row.id,
            operation_id: row.operation_id,
            reviewer: row.reviewer,
            model: row.model,
            summary: row.summary,
            content: row.content,
            score: row.score,
            duration_ms: row.duration_ms as u64,
            created_at: row.created_at,
        }
    }
}
