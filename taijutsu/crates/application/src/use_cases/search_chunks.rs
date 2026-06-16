//! Use Case: SearchChunks — Interroger la mémoire sémantique de l'IA.
//!
//! Expose les fragments sémantiques persistés dans PostgreSQL
//! via le port `ChunkRepository`. Permet aux clients (humains ou IA)
//! de rechercher des symboles par opération, fichier, nom, ou
//! **similarité sémantique** (RAG vectoriel, Phase 7A).
//!
//! ## Endpoints desservis
//! - `GET /api/v1/operations/:id/chunks` → tous les chunks d'une opération
//! - `GET /api/v1/operations/:id/chunks?file=...` → chunks d'un fichier
//! - `GET /api/v1/chunks/search?name=...` → recherche par nom de symbole
//! - `POST /api/v1/chunks/semantic-search` → recherche par similarité (RAG)
//! - gRPC: `GetChunksByOperation`, `SearchChunksByName`, `SemanticSearch`

use std::sync::Arc;

use tracing::{instrument, warn};
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::chunk_repository::{ChunkRepository, SimilarChunk, StoredChunk};
use domain::ports::embedding_service::EmbeddingService;

/// Filtre de recherche pour les chunks sémantiques.
#[derive(Debug)]
pub enum ChunkSearchFilter {
    /// Tous les chunks d'une opération.
    ByOperation { operation_id: Uuid },

    /// Chunks d'un fichier spécifique dans une opération.
    ByFile {
        operation_id: Uuid,
        file_path: String,
    },

    /// Recherche par nom de symbole (cross-opérations).
    ByName { name: String },

    /// Recherche par similarité sémantique (RAG vectoriel, Phase 7A).
    ///
    /// La requête en langage naturel est transformée en embedding via
    /// `EmbeddingService`, puis comparée aux chunks stockés par
    /// similarité cosinus (pgvector HNSW).
    BySemantic {
        /// Requête en langage naturel (ex: "code qui gère la cryptographie").
        query: String,
        /// Nombre maximum de résultats.
        limit: usize,
        /// Score minimum de similarité (0.0 à 1.0).
        threshold: f32,
    },
}

/// Résultat de recherche classique (mots-clés) avec métadonnées.
#[derive(Debug)]
pub struct ChunkSearchResult {
    /// Fragments sémantiques trouvés.
    pub chunks: Vec<StoredChunk>,
    /// Nombre total de résultats.
    pub count: usize,
}

/// Résultat de recherche sémantique (RAG) avec scores de similarité.
#[derive(Debug)]
pub struct SemanticSearchResult {
    /// Fragments sémantiques trouvés avec leurs scores de similarité.
    pub chunks: Vec<SimilarChunk>,
    /// Nombre total de résultats.
    pub count: usize,
}

/// Use case: interroger la mémoire sémantique de l'IA.
///
/// Supporte la recherche classique (Phase 6C) et la recherche
/// par similarité vectorielle (Phase 7A, RAG).
pub struct SearchChunksUseCase {
    chunk_repository: Arc<dyn ChunkRepository>,
    /// Phase 7A — Service d'embedding pour la recherche sémantique.
    /// `None` si le RAG est désactivé.
    embedding_service: Option<Arc<dyn EmbeddingService>>,
}

impl SearchChunksUseCase {
    /// Construit le use case avec les dépendances injectées.
    ///
    /// # Arguments
    /// - `chunk_repository` : persistence des chunks
    /// - `embedding_service` : embedding vectoriel pour la recherche sémantique
    ///   (`None` = filtre `BySemantic` retourne une erreur)
    pub fn new(
        chunk_repository: Arc<dyn ChunkRepository>,
        embedding_service: Option<Arc<dyn EmbeddingService>>,
    ) -> Self {
        Self {
            chunk_repository,
            embedding_service,
        }
    }

    /// Exécute une recherche de chunks selon le filtre fourni.
    ///
    /// Pour les filtres classiques (ByOperation, ByFile, ByName),
    /// retourne un `ChunkSearchResult`.
    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        filter: ChunkSearchFilter,
    ) -> Result<ChunkSearchResult, DomainError> {
        let chunks = match &filter {
            ChunkSearchFilter::ByOperation { operation_id } => {
                self.chunk_repository
                    .find_by_operation(operation_id)
                    .await?
            }
            ChunkSearchFilter::ByFile {
                operation_id,
                file_path,
            } => {
                self.chunk_repository
                    .find_by_file(operation_id, file_path)
                    .await?
            }
            ChunkSearchFilter::ByName { name } => {
                self.chunk_repository.find_by_name(name).await?
            }
            ChunkSearchFilter::BySemantic { .. } => {
                // Les requêtes sémantiques utilisent `execute_semantic()`.
                return Err(DomainError::Internal(
                    "Utiliser execute_semantic() pour les requêtes BySemantic".to_string(),
                ));
            }
        };

        let count = chunks.len();

        Ok(ChunkSearchResult { chunks, count })
    }

    /// Exécute une recherche sémantique (RAG vectoriel).
    ///
    /// ## Flux
    /// 1. La requête est préfixée avec `search_query:` (task prefix Nomic)
    /// 2. `EmbeddingService.embed()` transforme la requête en vecteur 256d
    /// 3. `ChunkRepository.search_similar()` exécute une recherche HNSW pgvector
    /// 4. Les résultats sont triés par similarité descendante
    #[instrument(skip(self))]
    pub async fn execute_semantic(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,
    ) -> Result<SemanticSearchResult, DomainError> {
        let embed_svc = self.embedding_service.as_ref().ok_or_else(|| {
            DomainError::Internal(
                "Recherche sémantique désactivée — EmbeddingService non disponible".to_string(),
            )
        })?;

        // 1. Préfixer avec search_query: pour Nomic.
        let prefixed_query = format!("search_query: {query}");

        // 2. Transformer la requête en embedding.
        let query_embedding = embed_svc.embed(&prefixed_query).await.map_err(|e| {
            warn!(error = %e, "⚠️ Embedding de la requête échoué");
            e
        })?;

        // 3. Recherche par similarité cosinus via pgvector.
        let chunks = self
            .chunk_repository
            .search_similar(&query_embedding, limit, threshold)
            .await?;

        let count = chunks.len();

        Ok(SemanticSearchResult { chunks, count })
    }

    /// Compte le nombre de chunks pour une opération (sans charger les données).
    #[instrument(skip(self))]
    pub async fn count(
        &self,
        operation_id: &Uuid,
    ) -> Result<usize, DomainError> {
        self.chunk_repository
            .count_by_operation(operation_id)
            .await
    }
}
