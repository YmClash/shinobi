//! Adaptateur PostgreSQL — CommitStatusRepository (Phase 39 — Le Pont CI/CD) 🌉
//!
//! Implémente le port `CommitStatusRepository` pour le stockage PostgreSQL.
//! Gère les statuts de commit CI/CD avec UPSERT sur la clé unique
//! `(repository_id, commit_id, context)`.
//!
//! ## Pattern
//! - `state` stocké en VARCHAR(20) avec CHECK constraint SQL
//! - UPSERT via `INSERT ... ON CONFLICT DO UPDATE`
//! - Row mapper privé pour isoler la couche SQL de la couche domaine

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use domain::entities::commit_status::{CommitStatus, CommitStatusState};
use domain::errors::DomainError;
use domain::ports::commit_status_repository::CommitStatusRepository;

/// Adaptateur PostgreSQL pour les statuts de commit CI/CD.
pub struct PostgresCommitStatusRepo {
    pool: PgPool,
}

impl PostgresCommitStatusRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommitStatusRepository for PostgresCommitStatusRepo {
    async fn upsert(&self, status: &CommitStatus) -> Result<CommitStatus, DomainError> {
        let row = sqlx::query_as::<_, CommitStatusRow>(
            r#"
            INSERT INTO commit_statuses (
                id, repository_id, commit_id, context, state,
                description, target_url, creator_id, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10
            )
            ON CONFLICT (repository_id, commit_id, context) DO UPDATE SET
                state = EXCLUDED.state,
                description = EXCLUDED.description,
                target_url = EXCLUDED.target_url,
                creator_id = EXCLUDED.creator_id,
                updated_at = EXCLUDED.updated_at
            RETURNING id, repository_id, commit_id, context, state,
                      description, target_url, creator_id,
                      created_at, updated_at
            "#,
        )
        .bind(status.id)
        .bind(status.repository_id)
        .bind(&status.commit_id)
        .bind(&status.context)
        .bind(status.state.as_sql_str())
        .bind(&status.description)
        .bind(&status.target_url)
        .bind(status.creator_id)
        .bind(status.created_at)
        .bind(status.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("commit_status upsert: {e}")))?;

        Ok(row.into_domain())
    }

    async fn list_by_commit(
        &self,
        repository_id: &Uuid,
        commit_id: &str,
    ) -> Result<Vec<CommitStatus>, DomainError> {
        let rows = sqlx::query_as::<_, CommitStatusRow>(
            r#"
            SELECT id, repository_id, commit_id, context, state,
                   description, target_url, creator_id,
                   created_at, updated_at
            FROM commit_statuses
            WHERE repository_id = $1 AND commit_id = $2
            ORDER BY updated_at DESC
            "#,
        )
        .bind(repository_id)
        .bind(commit_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("commit_status list_by_commit: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    async fn find_by_context(
        &self,
        repository_id: &Uuid,
        commit_id: &str,
        context: &str,
    ) -> Result<Option<CommitStatus>, DomainError> {
        let row = sqlx::query_as::<_, CommitStatusRow>(
            r#"
            SELECT id, repository_id, commit_id, context, state,
                   description, target_url, creator_id,
                   created_at, updated_at
            FROM commit_statuses
            WHERE repository_id = $1 AND commit_id = $2 AND context = $3
            "#,
        )
        .bind(repository_id)
        .bind(commit_id)
        .bind(context)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("commit_status find_by_context: {e}")))?;

        Ok(row.map(|r| r.into_domain()))
    }
}

// ── Row Mapper ────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct CommitStatusRow {
    id: Uuid,
    repository_id: Uuid,
    commit_id: String,
    context: String,
    state: String,
    description: Option<String>,
    target_url: Option<String>,
    creator_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl CommitStatusRow {
    fn into_domain(self) -> CommitStatus {
        CommitStatus {
            id: self.id,
            repository_id: self.repository_id,
            commit_id: self.commit_id,
            context: self.context,
            state: CommitStatusState::from_sql(&self.state)
                .unwrap_or(CommitStatusState::Pending),
            description: self.description,
            target_url: self.target_url,
            creator_id: self.creator_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
