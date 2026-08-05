//! PostgreSQL implementation du FederationRepository — Phase 27.
//!
//! Persiste les keypairs RSA et les follows fédérés dans PostgreSQL.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

use domain::entities::federation::{FederationActivity, FederationFollow, FederationKeypair};
use domain::errors::DomainError;
use domain::ports::federation_repository::FederationRepository;

/// Implémentation PostgreSQL du port FederationRepository.
pub struct PostgresFederationRepository {
    pool: PgPool,
}

impl PostgresFederationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FederationRepository for PostgresFederationRepository {
    async fn get_keypair(&self, actor_id: &Uuid) -> Result<Option<FederationKeypair>, DomainError> {
        let row = sqlx::query_as::<_, KeypairRow>(
            "SELECT actor_id, public_key_pem, private_key_pem, key_id, created_at
             FROM federation_keys WHERE actor_id = $1"
        )
        .bind(actor_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.map(|r| FederationKeypair {
            actor_id: r.actor_id,
            public_key_pem: r.public_key_pem,
            private_key_pem: r.private_key_pem,
            key_id: r.key_id,
            created_at: r.created_at,
        }))
    }

    async fn save_keypair(&self, keypair: &FederationKeypair) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO federation_keys (actor_id, public_key_pem, private_key_pem, key_id, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (actor_id) DO UPDATE SET
                public_key_pem = EXCLUDED.public_key_pem,
                private_key_pem = EXCLUDED.private_key_pem,
                key_id = EXCLUDED.key_id"
        )
        .bind(&keypair.actor_id)
        .bind(&keypair.public_key_pem)
        .bind(&keypair.private_key_pem)
        .bind(&keypair.key_id)
        .bind(&keypair.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        info!(
            actor_id = %keypair.actor_id,
            key_id = %keypair.key_id,
            "✅ Federation keypair saved"
        );
        Ok(())
    }

    async fn save_follow(&self, follow: &FederationFollow) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO federation_follows (id, follower_uri, following_actor_id, accepted, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (follower_uri, following_actor_id) DO UPDATE SET
                accepted = EXCLUDED.accepted"
        )
        .bind(&follow.id)
        .bind(&follow.follower_uri)
        .bind(&follow.following_actor_id)
        .bind(&follow.accepted)
        .bind(&follow.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    async fn delete_follow(&self, follower_uri: &str, following_actor_id: &Uuid) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "DELETE FROM federation_follows WHERE follower_uri = $1 AND following_actor_id = $2"
        )
        .bind(follower_uri)
        .bind(following_actor_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_followers(&self, actor_id: &Uuid) -> Result<Vec<FederationFollow>, DomainError> {
        let rows = sqlx::query_as::<_, FollowRow>(
            "SELECT id, follower_uri, following_actor_id, accepted, created_at
             FROM federation_follows
             WHERE following_actor_id = $1 AND accepted = TRUE
             ORDER BY created_at DESC"
        )
        .bind(actor_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(rows.into_iter().map(|r| FederationFollow {
            id: r.id,
            follower_uri: r.follower_uri,
            following_actor_id: r.following_actor_id,
            accepted: r.accepted,
            created_at: r.created_at,
        }).collect())
    }

    async fn count_followers(&self, actor_id: &Uuid) -> Result<i64, DomainError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM federation_follows WHERE following_actor_id = $1 AND accepted = TRUE"
        )
        .bind(actor_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.0)
    }

    // ── Activities (Outbox) — Phase 27-bis-D ──────────────────

    async fn save_activity(&self, activity: &FederationActivity) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO federation_activities (id, actor_id, activity_type, object_type, object_id, activity_json, published_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(&activity.id)
        .bind(&activity.actor_id)
        .bind(&activity.activity_type)
        .bind(&activity.object_type)
        .bind(&activity.object_id)
        .bind(&activity.activity_json)
        .bind(&activity.published_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    async fn list_activities(&self, actor_id: &Uuid, limit: i64) -> Result<Vec<FederationActivity>, DomainError> {
        let rows = sqlx::query_as::<_, ActivityRow>(
            "SELECT id, actor_id, activity_type, object_type, object_id, activity_json, published_at
             FROM federation_activities
             WHERE actor_id = $1
             ORDER BY published_at DESC
             LIMIT $2"
        )
        .bind(actor_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(rows.into_iter().map(|r| FederationActivity {
            id: r.id,
            actor_id: r.actor_id,
            activity_type: r.activity_type,
            object_type: r.object_type,
            object_id: r.object_id,
            activity_json: r.activity_json,
            published_at: r.published_at,
        }).collect())
    }

    async fn count_activities(&self, actor_id: &Uuid) -> Result<i64, DomainError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM federation_activities WHERE actor_id = $1"
        )
        .bind(actor_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.0)
    }

    // ── Stats (NodeInfo) ─────────────────────────────────────

    async fn count_local_users(&self) -> Result<i64, DomainError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM actors WHERE actor_type = 'human'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.0)
    }

    async fn count_local_repos(&self) -> Result<i64, DomainError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM repositories WHERE visibility = 'public' AND deleted_at IS NULL"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(row.0)
    }
}

// ── SQLx row types ──────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct KeypairRow {
    actor_id: Uuid,
    public_key_pem: String,
    private_key_pem: String,
    key_id: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct FollowRow {
    id: Uuid,
    follower_uri: String,
    following_actor_id: Uuid,
    accepted: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct ActivityRow {
    id: Uuid,
    actor_id: Uuid,
    activity_type: String,
    object_type: String,
    object_id: String,
    activity_json: serde_json::Value,
    published_at: chrono::DateTime<chrono::Utc>,
}
