//! Adaptateur Fūinjutsu — PostgreSQL RepoRepository.
//!
//! Implémentation concrète du port `RepoRepository`
//! utilisant PostgreSQL via sqlx pour la persistence des dépôts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::entities::repository::{Repository, Visibility};
use domain::errors::DomainError;
use domain::ports::repo_repository::RepoRepository;

/// Adaptateur PostgreSQL pour la persistence des dépôts.
#[derive(Debug, Clone)]
pub struct PostgresRepoRepository {
    pool: PgPool,
}

impl PostgresRepoRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit une entité `Repository` à partir d'une ligne PostgreSQL.
fn row_to_repository(row: sqlx::postgres::PgRow) -> Result<Repository, DomainError> {
    let visibility_str: String = row
        .try_get("visibility")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

    let visibility = Visibility::from_sql_str(&visibility_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown visibility: {visibility_str}"))
    })?;

    Ok(Repository {
        id: row
            .try_get("id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        owner_id: row
            .try_get("owner_id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        display_name: row
            .try_get("display_name")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        description: row
            .try_get("description")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        visibility,
        default_branch: row
            .try_get("default_branch")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        created_at: row
            .try_get::<DateTime<Utc>, _>("created_at")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        mirror_source_url: row
            .try_get("mirror_source_url")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        mirror_synced_at: row
            .try_get::<Option<DateTime<Utc>>, _>("mirror_synced_at")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

#[async_trait]
impl RepoRepository for PostgresRepoRepository {
    #[instrument(skip(self, repo), fields(repo_id = %repo.id, name = %repo.name))]
    async fn save(&self, repo: &Repository) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO repositories (id, owner_id, name, display_name, description, visibility, default_branch, created_at, mirror_source_url, mirror_synced_at)
            VALUES ($1, $2, $3, $4, $5, $6::visibility, $7, $8, $9, $10)
            "#,
        )
        .bind(repo.id)
        .bind(repo.owner_id)
        .bind(&repo.name)
        .bind(&repo.display_name)
        .bind(&repo.description)
        .bind(repo.visibility.as_sql_str())
        .bind(&repo.default_branch)
        .bind(repo.created_at)
        .bind(&repo.mirror_source_url)
        .bind(repo.mirror_synced_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique") {
                DomainError::Duplicate(format!(
                    "Dépôt '{}/{}' existe déjà",
                    repo.owner_id, repo.name
                ))
            } else {
                DomainError::Persistence(e.to_string())
            }
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Repository>, DomainError> {
        let row = sqlx::query(
            "SELECT id, owner_id, name, display_name, description, visibility::text, default_branch, created_at, mirror_source_url, mirror_synced_at \
             FROM repositories WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_repository(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn find_by_owner_and_name(
        &self,
        owner_id: &Uuid,
        name: &str,
    ) -> Result<Option<Repository>, DomainError> {
        let row = sqlx::query(
            "SELECT id, owner_id, name, display_name, description, visibility::text, default_branch, created_at, mirror_source_url, mirror_synced_at \
             FROM repositories WHERE owner_id = $1 AND name = $2",
        )
        .bind(owner_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_repository(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn list_by_owner(&self, owner_id: &Uuid) -> Result<Vec<Repository>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, owner_id, name, display_name, description, visibility::text, default_branch, created_at, mirror_source_url, mirror_synced_at \
             FROM repositories WHERE owner_id = $1 ORDER BY created_at DESC",
        )
        .bind(owner_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_repository).collect()
    }

    #[instrument(skip(self))]
    async fn list_public(&self, limit: usize) -> Result<Vec<Repository>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, owner_id, name, display_name, description, visibility::text, default_branch, created_at, mirror_source_url, mirror_synced_at \
             FROM repositories WHERE visibility = 'public' ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_repository).collect()
    }

    #[instrument(skip(self))]
    async fn add_collaborator(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
        role: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO collaborators (actor_id, repository_id, role)
            VALUES ($1, $2, $3::repo_role)
            ON CONFLICT (actor_id, repository_id) DO NOTHING
            "#,
        )
        .bind(actor_id)
        .bind(repo_id)
        .bind(role)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn is_collaborator(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
    ) -> Result<bool, DomainError> {
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM collaborators WHERE actor_id = $1 AND repository_id = $2) AS is_collab",
        )
        .bind(actor_id)
        .bind(repo_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.try_get::<bool, _>("is_collab").unwrap_or(false))
    }

    #[instrument(skip(self))]
    async fn get_role(
        &self,
        actor_id: &Uuid,
        repo_id: &Uuid,
    ) -> Result<Option<String>, DomainError> {
        let row = sqlx::query(
            "SELECT role::text FROM collaborators WHERE actor_id = $1 AND repository_id = $2",
        )
        .bind(actor_id)
        .bind(repo_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.and_then(|r| r.try_get::<String, _>("role").ok()))
    }

    #[instrument(skip(self))]
    async fn update_mirror_synced_at(&self, repo_id: &Uuid) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE repositories SET mirror_synced_at = NOW() WHERE id = $1",
        )
        .bind(repo_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }
}
