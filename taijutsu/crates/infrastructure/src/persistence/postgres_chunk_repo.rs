//! Adaptateur Fūinjutsu — PostgreSQL ChunkRepository.
//!
//! Mémoire permanente de l'agent IA Tensai.
//! Persiste les `StoredChunk` via `UNNEST` pour une insertion
//! batch haute performance, et expose des requêtes indexées
//! pour la recherche sémantique.
//!
//! ## Phase 7A — Évolution Vectorielle
//! - Colonne `embedding vector(256)` (Nomic Matryoshka) via pgvector
//! - Index HNSW pour la recherche par similarité cosinus O(log n)
//! - UNNEST avec 10 colonnes (9 existantes + embedding)
//!
//! ## Pattern UNNEST
//! Au lieu de construire une chaîne `VALUES (...), (...), ...` dynamique
//! (lent, risque d'injection), on passe des tableaux PostgreSQL en
//! paramètres et `UNNEST` les décompresse en lignes au niveau du
//! moteur C de PostgreSQL. C'est la méthode la plus rapide et la
//! plus sûre de l'écosystème sqlx.

use async_trait::async_trait;
use pgvector::Vector;
use sqlx::{PgPool, Row};
use tracing::instrument;
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::chunk_repository::{ChunkRepository, SimilarChunk, StoredChunk};

/// Adaptateur PostgreSQL pour la persistence des fragments sémantiques.
///
/// Partage le même `PgPool` que `PostgresOperationRepository`.
#[derive(Debug, Clone)]
pub struct PostgresChunkRepository {
    pool: PgPool,
}

impl PostgresChunkRepository {
    /// Construit un nouveau repository avec le pool de connexions fourni.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Reconstruit un `StoredChunk` à partir d'une ligne PostgreSQL.
fn row_to_chunk(row: sqlx::postgres::PgRow) -> Result<StoredChunk, DomainError> {
    // Lire l'embedding optionnel (nullable — chunks pré-Phase 7A).
    let embedding: Option<Vector> = row
        .try_get("embedding")
        .ok();

    Ok(StoredChunk {
        kind: row
            .try_get("kind")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        content: row
            .try_get("content")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        start_line: row
            .try_get::<i32, _>("start_line")
            .map_err(|e| DomainError::Persistence(e.to_string()))? as usize,
        end_line: row
            .try_get::<i32, _>("end_line")
            .map_err(|e| DomainError::Persistence(e.to_string()))? as usize,
        file_path: row
            .try_get("file_path")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        language: row
            .try_get("language")
            .map_err(|e| DomainError::Persistence(e.to_string()))?,
        embedding: embedding.map(|v| v.to_vec()),
    })
}

#[async_trait]
impl ChunkRepository for PostgresChunkRepository {
    #[instrument(skip(self, chunks), fields(operation_id = %operation_id, chunk_count = chunks.len()))]
    async fn save_chunks(
        &self,
        operation_id: &Uuid,
        chunks: &[StoredChunk],
    ) -> Result<usize, DomainError> {
        if chunks.is_empty() {
            return Ok(0);
        }

        // Préparer les colonnes pour UNNEST (10 colonnes avec embedding).
        let len = chunks.len();
        let mut ids: Vec<Uuid> = Vec::with_capacity(len);
        let mut op_ids: Vec<Uuid> = Vec::with_capacity(len);
        let mut kinds: Vec<String> = Vec::with_capacity(len);
        let mut names: Vec<Option<String>> = Vec::with_capacity(len);
        let mut contents: Vec<String> = Vec::with_capacity(len);
        let mut start_lines: Vec<i32> = Vec::with_capacity(len);
        let mut end_lines: Vec<i32> = Vec::with_capacity(len);
        let mut file_paths: Vec<String> = Vec::with_capacity(len);
        let mut languages: Vec<String> = Vec::with_capacity(len);
        let mut embeddings: Vec<Option<Vector>> = Vec::with_capacity(len);

        for chunk in chunks {
            ids.push(Uuid::new_v4());
            op_ids.push(*operation_id);
            kinds.push(chunk.kind.clone());
            names.push(chunk.name.clone());
            contents.push(chunk.content.clone());
            start_lines.push(chunk.start_line as i32);
            end_lines.push(chunk.end_line as i32);
            file_paths.push(chunk.file_path.clone());
            languages.push(chunk.language.clone());
            embeddings.push(chunk.embedding.as_ref().map(|e| Vector::from(e.clone())));
        }

        // UNNEST : insertion batch haute performance avec embedding.
        let result = sqlx::query(
            r#"
            INSERT INTO semantic_chunks (id, operation_id, kind, name, content, start_line, end_line, file_path, language, embedding)
            SELECT * FROM UNNEST(
                $1::uuid[],
                $2::uuid[],
                $3::text[],
                $4::text[],
                $5::text[],
                $6::int4[],
                $7::int4[],
                $8::text[],
                $9::text[],
                $10::vector[]
            )
            "#,
        )
        .bind(&ids)
        .bind(&op_ids)
        .bind(&kinds)
        .bind(&names)
        .bind(&contents)
        .bind(&start_lines)
        .bind(&end_lines)
        .bind(&file_paths)
        .bind(&languages)
        .bind(&embeddings)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("Batch insert chunks failed: {e}")))?;

        Ok(result.rows_affected() as usize)
    }

    #[instrument(skip(self))]
    async fn find_by_operation(
        &self,
        operation_id: &Uuid,
    ) -> Result<Vec<StoredChunk>, DomainError> {
        let rows = sqlx::query(
            "SELECT kind, name, content, start_line, end_line, file_path, language, embedding \
             FROM semantic_chunks WHERE operation_id = $1 \
             ORDER BY file_path, start_line",
        )
        .bind(operation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_chunk).collect()
    }

    #[instrument(skip(self))]
    async fn find_by_file(
        &self,
        operation_id: &Uuid,
        file_path: &str,
    ) -> Result<Vec<StoredChunk>, DomainError> {
        let rows = sqlx::query(
            "SELECT kind, name, content, start_line, end_line, file_path, language, embedding \
             FROM semantic_chunks WHERE operation_id = $1 AND file_path = $2 \
             ORDER BY start_line",
        )
        .bind(operation_id)
        .bind(file_path)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_chunk).collect()
    }

    #[instrument(skip(self))]
    async fn find_by_name(
        &self,
        name: &str,
    ) -> Result<Vec<StoredChunk>, DomainError> {
        let rows = sqlx::query(
            "SELECT kind, name, content, start_line, end_line, file_path, language, embedding \
             FROM semantic_chunks WHERE name = $1 \
             ORDER BY file_path, start_line",
        )
        .bind(name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        rows.into_iter().map(row_to_chunk).collect()
    }

    #[instrument(skip(self))]
    async fn count_by_operation(
        &self,
        operation_id: &Uuid,
    ) -> Result<usize, DomainError> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM semantic_chunks WHERE operation_id = $1",
        )
        .bind(operation_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(e.to_string()))?;

        let count: i64 = row
            .try_get("count")
            .map_err(|e| DomainError::Persistence(e.to_string()))?;

        Ok(count as usize)
    }

    #[instrument(skip(self, embedding), fields(embedding_dim = embedding.len(), limit, threshold))]
    async fn search_similar(
        &self,
        embedding: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<SimilarChunk>, DomainError> {
        let query_vector = Vector::from(embedding.to_vec());

        // Recherche par similarité cosinus via pgvector.
        // `1 - (embedding <=> $1)` convertit la distance cosinus en similarité (0→1).
        // L'index HNSW accélère le ORDER BY en O(log n).
        let rows = sqlx::query(
            r#"
            SELECT kind, name, content, start_line, end_line, file_path, language, embedding,
                   1 - (embedding <=> $1::vector) AS similarity
            FROM semantic_chunks
            WHERE embedding IS NOT NULL
            ORDER BY embedding <=> $1::vector
            LIMIT $2
            "#,
        )
        .bind(&query_vector)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("Semantic search failed: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let similarity: f64 = row
                .try_get("similarity")
                .map_err(|e| DomainError::Persistence(e.to_string()))?;

            // Filtrer par seuil en Rust (plus flexible que SQL).
            if similarity as f32 >= threshold {
                let chunk = row_to_chunk(row)?;
                results.push(SimilarChunk {
                    chunk,
                    similarity: similarity as f32,
                });
            }
        }

        Ok(results)
    }
}
