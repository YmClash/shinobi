//! Adaptateur Fūinjutsu — Redis Cache & Distributed Locking.
//!
//! Fournit le cache applicatif et les verrous distribués
//! nécessaires à la coordination des opérations VCS concurrentes.

use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use tracing::instrument;

/// Client Redis pour le cache et le locking distribué.
#[derive(Clone)]
pub struct RedisCache {
    connection: MultiplexedConnection,
}

impl RedisCache {
    /// Construit un nouveau client Redis à partir d'une connexion multiplexée.
    pub fn new(connection: MultiplexedConnection) -> Self {
        Self { connection }
    }

    /// Connecte à Redis via l'URL fournie.
    pub async fn connect(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_multiplexed_async_connection().await?;
        Ok(Self { connection })
    }

    /// Stocke une valeur avec expiration optionnelle (en secondes).
    #[instrument(skip(self, value))]
    pub async fn set(
        &self,
        key: &str,
        value: &str,
        ttl_seconds: Option<u64>,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.connection.clone();
        match ttl_seconds {
            Some(ttl) => {
                conn.set_ex::<_, _, ()>(key, value, ttl).await?;
            }
            None => {
                conn.set::<_, _, ()>(key, value).await?;
            }
        }
        Ok(())
    }

    /// Récupère une valeur par sa clé.
    #[instrument(skip(self))]
    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        let mut conn = self.connection.clone();
        let result: Option<String> = conn.get(key).await?;
        Ok(result)
    }

    /// Acquiert un verrou distribué (SET NX avec TTL).
    /// Retourne `true` si le verrou est acquis, `false` sinon.
    #[instrument(skip(self))]
    pub async fn acquire_lock(
        &self,
        lock_key: &str,
        lock_value: &str,
        ttl_seconds: u64,
    ) -> Result<bool, redis::RedisError> {
        let mut conn = self.connection.clone();
        let result: bool = redis::cmd("SET")
            .arg(lock_key)
            .arg(lock_value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_seconds)
            .query_async(&mut conn)
            .await
            .unwrap_or(false);
        Ok(result)
    }

    /// Libère un verrou distribué (seulement si la valeur correspond).
    #[instrument(skip(self))]
    pub async fn release_lock(
        &self,
        lock_key: &str,
        lock_value: &str,
    ) -> Result<bool, redis::RedisError> {
        let script = redis::Script::new(
            r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
            "#,
        );
        let mut conn = self.connection.clone();
        let result: i32 = script
            .key(lock_key)
            .arg(lock_value)
            .invoke_async(&mut conn)
            .await
            .unwrap_or(0);
        Ok(result == 1)
    }
}
