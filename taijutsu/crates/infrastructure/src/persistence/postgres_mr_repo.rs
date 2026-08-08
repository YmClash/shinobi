//! Adaptateur Fūinjutsu — PostgreSQL MrRepository.
//!
//! Implémentation concrète du port `MrRepository` pour la persistence
//! des Merge Requests, Reviews et Events dans PostgreSQL.
//!
//! ## Anti Race-Condition
//! `next_number()` utilise un UPSERT atomique sur `repo_counters`
//! pour garantir l'unicité des numéros de MR sous concurrence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::entities::merge_request::{
    MergeRequest, MrEvent, MrEventType, MrReview, MrStatus, MrVerdict,
};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;

/// Adaptateur PostgreSQL pour la persistence des Merge Requests.
#[derive(Debug, Clone)]
pub struct PostgresMrRepository {
    pool: PgPool,
}

impl PostgresMrRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit une `MergeRequest` depuis une ligne PostgreSQL.
fn row_to_mr(row: sqlx::postgres::PgRow) -> Result<MergeRequest, DomainError> {
    let status_str: String = row
        .try_get("status")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;
    let status = MrStatus::from_sql_str(&status_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown mr_status: {status_str}"))
    })?;

    Ok(MergeRequest {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        repository_id: row.try_get("repository_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        author_id: row.try_get("author_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        number: row.try_get("number").map_err(|e| DomainError::Persistence(e.to_string()))?,
        title: row.try_get("title").map_err(|e| DomainError::Persistence(e.to_string()))?,
        description: row.try_get("description").map_err(|e| DomainError::Persistence(e.to_string()))?,
        source_branch: row.try_get("source_branch").map_err(|e| DomainError::Persistence(e.to_string()))?,
        target_branch: row.try_get("target_branch").map_err(|e| DomainError::Persistence(e.to_string()))?,
        status,
        merged_by: row.try_get("merged_by").map_err(|e| DomainError::Persistence(e.to_string()))?,
        merged_at: row.try_get::<Option<DateTime<Utc>>, _>("merged_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
        closed_at: row.try_get::<Option<DateTime<Utc>>, _>("closed_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

/// Reconstruit un `MrReview` depuis une ligne PostgreSQL.
fn row_to_review(row: sqlx::postgres::PgRow) -> Result<MrReview, DomainError> {
    let verdict_str: String = row
        .try_get("verdict")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;
    let verdict = MrVerdict::from_sql_str(&verdict_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown mr_verdict: {verdict_str}"))
    })?;

    Ok(MrReview {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        mr_id: row.try_get("mr_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        reviewer_id: row.try_get("reviewer_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        verdict,
        body: row.try_get("body").map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

/// Reconstruit un `MrEvent` depuis une ligne PostgreSQL.
fn row_to_event(row: sqlx::postgres::PgRow) -> Result<MrEvent, DomainError> {
    let event_type_str: String = row
        .try_get("event_type")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;
    let event_type = MrEventType::from_sql_str(&event_type_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown mr_event_type: {event_type_str}"))
    })?;

    Ok(MrEvent {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        mr_id: row.try_get("mr_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        actor_id: row.try_get("actor_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        event_type,
        payload: row.try_get("payload").map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

#[async_trait]
impl MrRepository for PostgresMrRepository {
    // ── MergeRequest CRUD ───────────────────────────────────────────────

    #[instrument(skip(self, mr), fields(mr_id = %mr.id, repo_id = %mr.repository_id, number = mr.number))]
    async fn save(&self, mr: &MergeRequest) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO merge_requests (
                id, repository_id, author_id, number, title, description,
                source_branch, target_branch, status, merged_by, merged_at,
                closed_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::mr_status, $10, $11, $12, $13, $14)
            "#,
        )
        .bind(mr.id)
        .bind(mr.repository_id)
        .bind(mr.author_id)
        .bind(mr.number)
        .bind(&mr.title)
        .bind(&mr.description)
        .bind(&mr.source_branch)
        .bind(&mr.target_branch)
        .bind(mr.status.as_sql_str())
        .bind(mr.merged_by)
        .bind(mr.merged_at)
        .bind(mr.closed_at)
        .bind(mr.created_at)
        .bind(mr.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<MergeRequest>, DomainError> {
        let row = sqlx::query(
            "SELECT id, repository_id, author_id, number, title, description, \
             source_branch, target_branch, status::text, merged_by, merged_at, \
             closed_at, created_at, updated_at \
             FROM merge_requests WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_mr(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn find_by_repo_and_number(
        &self,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<Option<MergeRequest>, DomainError> {
        let row = sqlx::query(
            "SELECT id, repository_id, author_id, number, title, description, \
             source_branch, target_branch, status::text, merged_by, merged_at, \
             closed_at, created_at, updated_at \
             FROM merge_requests WHERE repository_id = $1 AND number = $2",
        )
        .bind(repo_id)
        .bind(number)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_mr(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn list_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<MrStatus>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MergeRequest>, DomainError> {
        let rows = if let Some(s) = status {
            sqlx::query(
                "SELECT id, repository_id, author_id, number, title, description, \
                 source_branch, target_branch, status::text, merged_by, merged_at, \
                 closed_at, created_at, updated_at \
                 FROM merge_requests \
                 WHERE repository_id = $1 AND status = $2::mr_status \
                 ORDER BY number DESC LIMIT $3 OFFSET $4",
            )
            .bind(repo_id)
            .bind(s.as_sql_str())
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT id, repository_id, author_id, number, title, description, \
                 source_branch, target_branch, status::text, merged_by, merged_at, \
                 closed_at, created_at, updated_at \
                 FROM merge_requests \
                 WHERE repository_id = $1 \
                 ORDER BY number DESC LIMIT $2 OFFSET $3",
            )
            .bind(repo_id)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_mr).collect()
    }

    #[instrument(skip(self))]
    async fn update_status(
        &self,
        id: &Uuid,
        status: MrStatus,
        merged_by: Option<Uuid>,
        merged_at: Option<DateTime<Utc>>,
        closed_at: Option<DateTime<Utc>>,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE merge_requests \
             SET status = $2::mr_status, merged_by = $3, merged_at = $4, \
                 closed_at = $5, updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(status.as_sql_str())
        .bind(merged_by)
        .bind(merged_at)
        .bind(closed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn next_number(&self, repo_id: &Uuid) -> Result<i32, DomainError> {
        // UPSERT atomique sur le compteur unifié (partagé Issues + MRs — Phase 33).
        // La première MR/issue d'un repo crée la ligne,
        // les suivantes incrémentent. RETURNING retourne le numéro attribué.
        let row = sqlx::query(
            r#"
            INSERT INTO repo_counters (repository_id, next_mr_number, next_ticket_number)
            VALUES ($1, 1, 2)
            ON CONFLICT (repository_id)
            DO UPDATE SET next_ticket_number = repo_counters.next_ticket_number + 1
            RETURNING next_ticket_number - 1 AS assigned_number
            "#,
        )
        .bind(repo_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        row.try_get::<i32, _>("assigned_number")
            .map_err(|e| DomainError::Persistence(e.to_string()))
    }

    #[instrument(skip(self))]
    async fn find_open_by_branches(
        &self,
        repo_id: &Uuid,
        source: &str,
        target: &str,
    ) -> Result<Option<MergeRequest>, DomainError> {
        let row = sqlx::query(
            "SELECT id, repository_id, author_id, number, title, description, \
             source_branch, target_branch, status::text, merged_by, merged_at, \
             closed_at, created_at, updated_at \
             FROM merge_requests \
             WHERE repository_id = $1 AND source_branch = $2 AND target_branch = $3 \
                   AND status = 'open'",
        )
        .bind(repo_id)
        .bind(source)
        .bind(target)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_mr(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn count_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<MrStatus>,
    ) -> Result<i64, DomainError> {
        let row = if let Some(s) = status {
            sqlx::query(
                "SELECT COUNT(*) as count FROM merge_requests \
                 WHERE repository_id = $1 AND status = $2::mr_status",
            )
            .bind(repo_id)
            .bind(s.as_sql_str())
            .fetch_one(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT COUNT(*) as count FROM merge_requests WHERE repository_id = $1",
            )
            .bind(repo_id)
            .fetch_one(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        row.try_get::<i64, _>("count")
            .map_err(|e| DomainError::Persistence(e.to_string()))
    }

    // ── Reviews ─────────────────────────────────────────────────────────

    #[instrument(skip(self, review), fields(review_id = %review.id, mr_id = %review.mr_id))]
    async fn save_review(&self, review: &MrReview) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO mr_reviews (id, mr_id, reviewer_id, verdict, body, created_at)
            VALUES ($1, $2, $3, $4::mr_verdict, $5, $6)
            "#,
        )
        .bind(review.id)
        .bind(review.mr_id)
        .bind(review.reviewer_id)
        .bind(review.verdict.as_sql_str())
        .bind(&review.body)
        .bind(review.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_reviews(&self, mr_id: &Uuid) -> Result<Vec<MrReview>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, mr_id, reviewer_id, verdict::text, body, created_at \
             FROM mr_reviews WHERE mr_id = $1 ORDER BY created_at ASC",
        )
        .bind(mr_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_review).collect()
    }

    // ── Events (Timeline) ───────────────────────────────────────────────

    #[instrument(skip(self, event), fields(event_id = %event.id, mr_id = %event.mr_id))]
    async fn save_event(&self, event: &MrEvent) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO mr_events (id, mr_id, actor_id, event_type, payload, created_at)
            VALUES ($1, $2, $3, $4::mr_event_type, $5, $6)
            "#,
        )
        .bind(event.id)
        .bind(event.mr_id)
        .bind(event.actor_id)
        .bind(event.event_type.as_sql_str())
        .bind(&event.payload)
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_events(&self, mr_id: &Uuid) -> Result<Vec<MrEvent>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, mr_id, actor_id, event_type::text, payload, created_at \
             FROM mr_events WHERE mr_id = $1 ORDER BY created_at ASC",
        )
        .bind(mr_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_event).collect()
    }
}
