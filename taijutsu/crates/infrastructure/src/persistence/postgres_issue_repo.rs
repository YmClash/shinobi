//! Adaptateur Fūinjutsu — PostgreSQL IssueRepository.
//!
//! Implémentation concrète du port `IssueRepository` pour la persistence
//! des Issues, Comments, Events et Labels dans PostgreSQL.
//!
//! ## Compteur Partagé
//! `next_number()` utilise le compteur unifié `repo_counters.next_ticket_number`
//! partagé avec les Merge Requests — UPSERT atomique anti race-condition.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::entities::issue::{
    Issue, IssueComment, IssueEvent, IssueEventType, IssueLabel, IssueStatus,
};
use domain::errors::DomainError;
use domain::ports::issue_repository::IssueRepository;

/// Adaptateur PostgreSQL pour la persistence des Issues.
#[derive(Debug, Clone)]
pub struct PostgresIssueRepository {
    pool: PgPool,
}

impl PostgresIssueRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit une `Issue` depuis une ligne PostgreSQL.
fn row_to_issue(row: sqlx::postgres::PgRow) -> Result<Issue, DomainError> {
    let status_str: String = row
        .try_get("status")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;
    let status = IssueStatus::from_sql_str(&status_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown issue_status: {status_str}"))
    })?;

    Ok(Issue {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        repository_id: row.try_get("repository_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        author_id: row.try_get("author_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        number: row.try_get("number").map_err(|e| DomainError::Persistence(e.to_string()))?,
        title: row.try_get("title").map_err(|e| DomainError::Persistence(e.to_string()))?,
        body: row.try_get("body").map_err(|e| DomainError::Persistence(e.to_string()))?,
        status,
        assignee_id: row.try_get("assignee_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        closed_by: row.try_get("closed_by").map_err(|e| DomainError::Persistence(e.to_string()))?,
        closed_at: row.try_get::<Option<DateTime<Utc>>, _>("closed_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

/// Reconstruit un `IssueComment` depuis une ligne PostgreSQL.
fn row_to_comment(row: sqlx::postgres::PgRow) -> Result<IssueComment, DomainError> {
    Ok(IssueComment {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        issue_id: row.try_get("issue_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        author_id: row.try_get("author_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        body: row.try_get("body").map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

/// Reconstruit un `IssueEvent` depuis une ligne PostgreSQL.
fn row_to_event(row: sqlx::postgres::PgRow) -> Result<IssueEvent, DomainError> {
    let event_type_str: String = row
        .try_get("event_type")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;
    let event_type = IssueEventType::from_sql_str(&event_type_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown issue_event_type: {event_type_str}"))
    })?;

    Ok(IssueEvent {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        issue_id: row.try_get("issue_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        actor_id: row.try_get("actor_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        event_type,
        payload: row.try_get("payload").map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

/// Reconstruit un `IssueLabel` depuis une ligne PostgreSQL.
fn row_to_label(row: sqlx::postgres::PgRow) -> Result<IssueLabel, DomainError> {
    Ok(IssueLabel {
        id: row.try_get("id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        repository_id: row.try_get("repository_id").map_err(|e| DomainError::Persistence(e.to_string()))?,
        name: row.try_get("name").map_err(|e| DomainError::Persistence(e.to_string()))?,
        color: row.try_get("color").map_err(|e| DomainError::Persistence(e.to_string()))?,
        description: row.try_get("description").map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

#[async_trait]
impl IssueRepository for PostgresIssueRepository {
    // ── Issue CRUD ──────────────────────────────────────────────────────

    #[instrument(skip(self, issue), fields(issue_id = %issue.id, repo_id = %issue.repository_id, number = issue.number))]
    async fn save(&self, issue: &Issue) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO issues (
                id, repository_id, author_id, number, title, body,
                status, assignee_id, closed_by, closed_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7::issue_status, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(issue.id)
        .bind(issue.repository_id)
        .bind(issue.author_id)
        .bind(issue.number)
        .bind(&issue.title)
        .bind(&issue.body)
        .bind(issue.status.as_sql_str())
        .bind(issue.assignee_id)
        .bind(issue.closed_by)
        .bind(issue.closed_at)
        .bind(issue.created_at)
        .bind(issue.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Issue>, DomainError> {
        let row = sqlx::query(
            "SELECT id, repository_id, author_id, number, title, body, \
             status::text, assignee_id, closed_by, closed_at, created_at, updated_at \
             FROM issues WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_issue(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn find_by_repo_and_number(
        &self,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<Option<Issue>, DomainError> {
        let row = sqlx::query(
            "SELECT id, repository_id, author_id, number, title, body, \
             status::text, assignee_id, closed_by, closed_at, created_at, updated_at \
             FROM issues WHERE repository_id = $1 AND number = $2",
        )
        .bind(repo_id)
        .bind(number)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_issue(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn list_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<IssueStatus>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Issue>, DomainError> {
        let rows = if let Some(s) = status {
            sqlx::query(
                "SELECT id, repository_id, author_id, number, title, body, \
                 status::text, assignee_id, closed_by, closed_at, created_at, updated_at \
                 FROM issues \
                 WHERE repository_id = $1 AND status = $2::issue_status \
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
                "SELECT id, repository_id, author_id, number, title, body, \
                 status::text, assignee_id, closed_by, closed_at, created_at, updated_at \
                 FROM issues \
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

        rows.into_iter().map(row_to_issue).collect()
    }

    #[instrument(skip(self))]
    async fn update_status(
        &self,
        id: &Uuid,
        status: IssueStatus,
        closed_by: Option<Uuid>,
        closed_at: Option<DateTime<Utc>>,
    ) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE issues \
             SET status = $2::issue_status, closed_by = $3, closed_at = $4, \
                 updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(status.as_sql_str())
        .bind(closed_by)
        .bind(closed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn update_title(&self, id: &Uuid, title: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE issues SET title = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(title)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn update_body(&self, id: &Uuid, body: Option<&str>) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE issues SET body = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(body)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn update_assignee(&self, id: &Uuid, assignee_id: Option<Uuid>) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE issues SET assignee_id = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(assignee_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn next_number(&self, repo_id: &Uuid) -> Result<i32, DomainError> {
        // UPSERT atomique sur le compteur unifié (partagé Issues + MRs).
        // La première issue/MR d'un repo crée la ligne,
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
    async fn count_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<IssueStatus>,
    ) -> Result<i64, DomainError> {
        let row = if let Some(s) = status {
            sqlx::query(
                "SELECT COUNT(*) as count FROM issues \
                 WHERE repository_id = $1 AND status = $2::issue_status",
            )
            .bind(repo_id)
            .bind(s.as_sql_str())
            .fetch_one(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT COUNT(*) as count FROM issues WHERE repository_id = $1",
            )
            .bind(repo_id)
            .fetch_one(&self.pool)
            .await
        }
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        row.try_get::<i64, _>("count")
            .map_err(|e| DomainError::Persistence(e.to_string()))
    }

    // ── Comments ────────────────────────────────────────────────────────

    #[instrument(skip(self, comment), fields(comment_id = %comment.id, issue_id = %comment.issue_id))]
    async fn save_comment(&self, comment: &IssueComment) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO issue_comments (id, issue_id, author_id, body, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(comment.id)
        .bind(comment.issue_id)
        .bind(comment.author_id)
        .bind(&comment.body)
        .bind(comment.created_at)
        .bind(comment.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_comments(&self, issue_id: &Uuid) -> Result<Vec<IssueComment>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, issue_id, author_id, body, created_at, updated_at \
             FROM issue_comments WHERE issue_id = $1 ORDER BY created_at ASC",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_comment).collect()
    }

    #[instrument(skip(self))]
    async fn update_comment(&self, comment_id: &Uuid, body: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE issue_comments SET body = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(comment_id)
        .bind(body)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn delete_comment(&self, comment_id: &Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "DELETE FROM issue_comments WHERE id = $1",
        )
        .bind(comment_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    // ── Events (Timeline) ───────────────────────────────────────────────

    #[instrument(skip(self, event), fields(event_id = %event.id, issue_id = %event.issue_id))]
    async fn save_event(&self, event: &IssueEvent) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO issue_events (id, issue_id, actor_id, event_type, payload, created_at)
            VALUES ($1, $2, $3, $4::issue_event_type, $5, $6)
            "#,
        )
        .bind(event.id)
        .bind(event.issue_id)
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
    async fn list_events(&self, issue_id: &Uuid) -> Result<Vec<IssueEvent>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, issue_id, actor_id, event_type::text, payload, created_at \
             FROM issue_events WHERE issue_id = $1 ORDER BY created_at ASC",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_event).collect()
    }

    // ── Labels ──────────────────────────────────────────────────────────

    #[instrument(skip(self, label), fields(label_id = %label.id, repo_id = %label.repository_id))]
    async fn save_label(&self, label: &IssueLabel) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO issue_labels (id, repository_id, name, color, description)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(label.id)
        .bind(label.repository_id)
        .bind(&label.name)
        .bind(&label.color)
        .bind(&label.description)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_labels(&self, repo_id: &Uuid) -> Result<Vec<IssueLabel>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, repository_id, name, color, description \
             FROM issue_labels WHERE repository_id = $1 ORDER BY name ASC",
        )
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_label).collect()
    }

    #[instrument(skip(self))]
    async fn delete_label(&self, label_id: &Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM issue_labels WHERE id = $1")
            .bind(label_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    #[instrument(skip(self))]
    async fn add_label_to_issue(&self, issue_id: &Uuid, label_id: &Uuid) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO issue_label_assignments (issue_id, label_id) \
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(issue_id)
        .bind(label_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn remove_label_from_issue(&self, issue_id: &Uuid, label_id: &Uuid) -> Result<(), DomainError> {
        sqlx::query(
            "DELETE FROM issue_label_assignments WHERE issue_id = $1 AND label_id = $2",
        )
        .bind(issue_id)
        .bind(label_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_issue_labels(&self, issue_id: &Uuid) -> Result<Vec<IssueLabel>, DomainError> {
        let rows = sqlx::query(
            "SELECT l.id, l.repository_id, l.name, l.color, l.description \
             FROM issue_labels l \
             INNER JOIN issue_label_assignments a ON a.label_id = l.id \
             WHERE a.issue_id = $1 \
             ORDER BY l.name ASC",
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_label).collect()
    }
}
