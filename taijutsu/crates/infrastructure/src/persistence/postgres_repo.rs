//! Adaptateur Fūinjutsu — PostgreSQL OperationRepository.
//!
//! Implémentation concrète du port `OperationRepository`
//! utilisant PostgreSQL via sqlx pour la persistence asynchrone.

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;

/// Adaptateur PostgreSQL pour la persistence des opérations.
///
/// Encapsule un pool de connexions `sqlx::PgPool`.
#[derive(Debug, Clone)]
pub struct PostgresOperationRepository {
    pool: PgPool,
}

impl PostgresOperationRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OperationRepository for PostgresOperationRepository {
    #[instrument(skip(self, operation), fields(operation_id = %operation.id))]
    async fn save(&self, operation: &Operation) -> Result<(), DomainError> {
        let parent_ids_json = serde_json::to_value(&operation.parent_ids)
            .map_err(|e| DomainError::Persistence(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO operations (id, author_id, content_id, description, parent_ids, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(operation.id)
        .bind(operation.author_id)
        .bind(operation.content_id.as_str())
        .bind(&operation.description)
        .bind(&parent_ids_json)
        .bind(operation.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self))]
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Operation>, DomainError> {
        // TODO: Implémenter la requête SELECT complète avec mapping.
        // Stub pour la phase d'initialisation.
        tracing::debug!(%id, "find_by_id: stub — sera implémenté en phase 2");
        Ok(None)
    }

    #[instrument(skip(self))]
    async fn list_recent(&self, limit: usize) -> Result<Vec<Operation>, DomainError> {
        // TODO: SELECT ... ORDER BY created_at DESC LIMIT $1
        tracing::debug!(limit, "list_recent: stub — sera implémenté en phase 2");
        Ok(Vec::new())
    }

    #[instrument(skip(self))]
    async fn find_by_author(&self, author_id: &Uuid) -> Result<Vec<Operation>, DomainError> {
        // TODO: SELECT ... WHERE author_id = $1
        tracing::debug!(%author_id, "find_by_author: stub — sera implémenté en phase 2");
        Ok(Vec::new())
    }
}
