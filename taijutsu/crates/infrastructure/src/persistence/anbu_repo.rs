//! Adaptateur Fūinjutsu — PostgreSQL AnbuRepository.
//!
//! Implémentation concrète du port `AnbuRepository`
//! pour la persistence des checkpoints ANBU.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::entities::anbu_checkpoint::AnbuCheckpoint;
use domain::errors::DomainError;
use domain::ports::anbu_repository::AnbuRepository;

/// Adaptateur PostgreSQL pour les checkpoints ANBU.
#[derive(Debug, Clone)]
pub struct PostgresAnbuRepository {
    pool: PgPool,
}

impl PostgresAnbuRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit un `AnbuCheckpoint` à partir d'une ligne PostgreSQL.
fn row_to_checkpoint(row: sqlx::postgres::PgRow) -> Result<AnbuCheckpoint, DomainError> {
    Ok(AnbuCheckpoint {
        id: row
            .try_get("id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        repository_id: row
            .try_get("repository_id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        actor_id: row
            .try_get("actor_id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        agent: row
            .try_get("agent")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        session_id: row
            .try_get("session_id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        message: row
            .try_get("message")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        commit_id: row
            .try_get("commit_id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        ipfs_cid: row
            .try_get("ipfs_cid")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        artifact_count: row
            .try_get("artifact_count")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        total_size: row
            .try_get("total_size")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row
            .try_get::<DateTime<Utc>, _>("created_at")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

#[async_trait]
impl AnbuRepository for PostgresAnbuRepository {
    #[instrument(skip(self, checkpoint), fields(checkpoint_id = %checkpoint.id))]
    async fn save_checkpoint(&self, checkpoint: &AnbuCheckpoint) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO anbu_checkpoints
                (id, repository_id, actor_id, agent, session_id, message,
                 commit_id, ipfs_cid, artifact_count, total_size, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                ipfs_cid = EXCLUDED.ipfs_cid,
                artifact_count = EXCLUDED.artifact_count,
                total_size = EXCLUDED.total_size
            "#,
        )
        .bind(checkpoint.id)
        .bind(checkpoint.repository_id)
        .bind(checkpoint.actor_id)
        .bind(&checkpoint.agent)
        .bind(&checkpoint.session_id)
        .bind(&checkpoint.message)
        .bind(&checkpoint.commit_id)
        .bind(&checkpoint.ipfs_cid)
        .bind(checkpoint.artifact_count)
        .bind(checkpoint.total_size)
        .bind(checkpoint.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_checkpoints(
        &self,
        repository_id: &Uuid,
        limit: usize,
    ) -> Result<Vec<AnbuCheckpoint>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, repository_id, actor_id, agent, session_id, message, \
             commit_id, ipfs_cid, artifact_count, total_size, created_at \
             FROM anbu_checkpoints WHERE repository_id = $1 \
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(repository_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_checkpoint).collect()
    }

    #[instrument(skip(self))]
    async fn find_checkpoint(&self, id: &Uuid) -> Result<Option<AnbuCheckpoint>, DomainError> {
        let row = sqlx::query(
            "SELECT id, repository_id, actor_id, agent, session_id, message, \
             commit_id, ipfs_cid, artifact_count, total_size, created_at \
             FROM anbu_checkpoints WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_checkpoint(r)?)),
            None => Ok(None),
        }
    }
}
