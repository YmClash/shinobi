//! Adaptateur Fūinjutsu — PostgreSQL ActorRepository.
//!
//! Implémentation concrète du port `ActorRepository`
//! utilisant PostgreSQL via sqlx pour la persistence asynchrone.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::entities::actor::{Actor, ActorType};
use domain::errors::DomainError;
use domain::ports::actor_repository::{ActorRepository, PatInfo};

/// Adaptateur PostgreSQL pour la persistence des acteurs.
#[derive(Debug, Clone)]
pub struct PostgresActorRepository {
    pool: PgPool,
}

impl PostgresActorRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit une entité `Actor` à partir d'une ligne PostgreSQL.
fn row_to_actor(row: sqlx::postgres::PgRow) -> Result<Actor, DomainError> {
    let actor_type_str: String = row
        .try_get("actor_type")
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

    let actor_type = ActorType::from_sql_str(&actor_type_str).ok_or_else(|| {
        DomainError::Persistence(format!("Unknown actor_type: {actor_type_str}"))
    })?;

    Ok(Actor {
        id: row
            .try_get("id")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        handle: row
            .try_get("handle")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        display_name: row
            .try_get("display_name")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        actor_type,
        avatar_url: row
            .try_get("avatar_url")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        email: row
            .try_get("email")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        bio: row
            .try_get("bio")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        github_id: row
            .try_get::<Option<i64>, _>("github_id")
            .unwrap_or(None),
        created_at: row
            .try_get::<DateTime<Utc>, _>("created_at")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
    })
}

#[async_trait]
impl ActorRepository for PostgresActorRepository {
    #[instrument(skip(self, actor), fields(actor_id = %actor.id, handle = %actor.handle))]
    async fn save(&self, actor: &Actor) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO actors (id, handle, display_name, actor_type, avatar_url, email, bio, github_id, created_at)
            VALUES ($1, $2, $3, $4::actor_type, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(actor.id)
        .bind(&actor.handle)
        .bind(&actor.display_name)
        .bind(actor.actor_type.as_sql_str())
        .bind(&actor.avatar_url)
        .bind(&actor.email)
        .bind(&actor.bio)
        .bind(actor.github_id)
        .bind(actor.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique") {
                DomainError::Duplicate(format!("Handle '{}' déjà pris", actor.handle))
            } else {
                DomainError::Persistence(e.to_string())
            }
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Actor>, DomainError> {
        let row = sqlx::query(
            "SELECT id, handle, display_name, actor_type::text, avatar_url, email, bio, github_id, created_at \
             FROM actors WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_actor(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn find_by_handle(&self, handle: &str) -> Result<Option<Actor>, DomainError> {
        let row = sqlx::query(
            "SELECT id, handle, display_name, actor_type::text, avatar_url, email, bio, github_id, created_at \
             FROM actors WHERE handle = $1",
        )
        .bind(handle)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_actor(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn find_by_email(&self, email: &str) -> Result<Option<Actor>, DomainError> {
        let row = sqlx::query(
            "SELECT a.id, a.handle, a.display_name, a.actor_type::text, a.avatar_url, a.email, a.bio, a.github_id, a.created_at \
             FROM actors a WHERE a.email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_actor(r)?)),
            None => Ok(None),
        }
    }

    // ── Credentials (Phase 19A) ─────────────────────────

    #[instrument(skip(self, secret_hash))]
    async fn save_credential(
        &self,
        actor_id: &Uuid,
        cred_type: &str,
        secret_hash: &str,
        email: Option<&str>,
        label: Option<&str>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            INSERT INTO credentials (actor_id, cred_type, secret_hash, email, label)
            VALUES ($1, $2::credential_type, $3, $4, $5)
            "#,
        )
        .bind(actor_id)
        .bind(cred_type)
        .bind(secret_hash)
        .bind(email)
        .bind(label)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                DomainError::Duplicate("Credential already exists".to_string())
            } else {
                DomainError::Persistence(e.to_string())
            }
        })?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_credential_hash(
        &self,
        actor_id: &Uuid,
        cred_type: &str,
    ) -> Result<Option<String>, DomainError> {
        let row = sqlx::query(
            "SELECT secret_hash FROM credentials \
             WHERE actor_id = $1 AND cred_type = $2::credential_type \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(actor_id)
        .bind(cred_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.map(|r| r.try_get::<String, _>("secret_hash").unwrap_or_default()))
    }

    #[instrument(skip(self))]
    async fn find_all_credential_hashes(
        &self,
        actor_id: &Uuid,
        cred_type: &str,
    ) -> Result<Vec<String>, DomainError> {
        let rows = sqlx::query(
            "SELECT secret_hash FROM credentials \
             WHERE actor_id = $1 AND cred_type = $2::credential_type",
        )
        .bind(actor_id)
        .bind(cred_type)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get::<String, _>("secret_hash").ok())
            .collect())
    }

    #[instrument(skip(self))]
    async fn find_actor_by_credential_hash(
        &self,
        secret_hash: &str,
        cred_type: &str,
    ) -> Result<Option<Actor>, DomainError> {
        let row = sqlx::query(
            "SELECT a.id, a.handle, a.display_name, a.actor_type::text, \
                    a.avatar_url, a.email, a.bio, a.github_id, a.created_at \
             FROM actors a \
             INNER JOIN credentials c ON c.actor_id = a.id \
             WHERE c.secret_hash = $1 AND c.cred_type = $2::credential_type \
             LIMIT 1",
        )
        .bind(secret_hash)
        .bind(cred_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_actor(r)?)),
            None => Ok(None),
        }
    }

    // ── GitHub OAuth (Phase 20) ──────────────────────────

    #[instrument(skip(self))]
    async fn find_by_github_id(&self, github_id: i64) -> Result<Option<Actor>, DomainError> {
        let row = sqlx::query(
            "SELECT id, handle, display_name, actor_type::text, avatar_url, email, bio, github_id, created_at \
             FROM actors WHERE github_id = $1",
        )
        .bind(github_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        match row {
            Some(r) => Ok(Some(row_to_actor(r)?)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self))]
    async fn update_github_id(&self, actor_id: &Uuid, github_id: i64) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE actors SET github_id = $1 WHERE id = $2",
        )
        .bind(github_id)
        .bind(actor_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_pats(&self, actor_id: &Uuid) -> Result<Vec<PatInfo>, DomainError> {
        let rows = sqlx::query(
            "SELECT id, label, created_at FROM credentials \
             WHERE actor_id = $1 AND cred_type = 'api_key' \
             ORDER BY created_at DESC",
        )
        .bind(actor_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(PatInfo {
                    id: r.try_get("id").ok()?,
                    label: r.try_get("label").ok()?,
                    created_at: r.try_get("created_at").ok()?,
                })
            })
            .collect())
    }
}
