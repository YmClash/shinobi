//! Use Case: SearchChunks — Interroger la mémoire sémantique de l'IA.
//!
//! Expose les fragments sémantiques persistés dans PostgreSQL
//! via le port `ChunkRepository`. Permet aux clients (humains ou IA)
//! de rechercher des symboles par opération, fichier, ou nom.
//!
//! ## Endpoints desservis
//! - `GET /api/v1/operations/:id/chunks` → tous les chunks d'une opération
//! - `GET /api/v1/operations/:id/chunks?file=...` → chunks d'un fichier
//! - `GET /api/v1/chunks/search?name=...` → recherche par nom de symbole
//! - gRPC: `GetChunksByOperation`, `SearchChunksByName`

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::chunk_repository::{ChunkRepository, StoredChunk};

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
}

/// Résultat de recherche avec métadonnées.
#[derive(Debug)]
pub struct ChunkSearchResult {
    /// Fragments sémantiques trouvés.
    pub chunks: Vec<StoredChunk>,
    /// Nombre total de résultats.
    pub count: usize,
}

/// Use case: interroger la mémoire sémantique de l'IA.
pub struct SearchChunksUseCase {
    chunk_repository: Arc<dyn ChunkRepository>,
}

impl SearchChunksUseCase {
    /// Construit le use case avec le repository injecté.
    pub fn new(chunk_repository: Arc<dyn ChunkRepository>) -> Self {
        Self { chunk_repository }
    }

    /// Exécute une recherche de chunks selon le filtre fourni.
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
        };

        let count = chunks.len();

        Ok(ChunkSearchResult { chunks, count })
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
