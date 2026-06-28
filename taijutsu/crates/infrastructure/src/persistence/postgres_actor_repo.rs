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
use domain::ports::actor_repository::ActorRepository;

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
        bio: row
            .try_get("bio")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
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
            INSERT INTO actors (id, handle, display_name, actor_type, avatar_url, bio, created_at)
            VALUES ($1, $2, $3, $4::actor_type, $5, $6, $7)
            "#,
        )
        .bind(actor.id)
        .bind(&actor.handle)
        .bind(&actor.display_name)
        .bind(actor.actor_type.as_sql_str())
        .bind(&actor.avatar_url)
        .bind(&actor.bio)
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
            "SELECT id, handle, display_name, actor_type::text, avatar_url, bio, created_at \
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
            "SELECT id, handle, display_name, actor_type::text, avatar_url, bio, created_at \
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
}
