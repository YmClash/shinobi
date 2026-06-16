//! Services gRPC Ninpo — Implémentation du service ShinobiService.
//!
//! Adaptateur primaire gRPC utilisant Tonic.
//! Les requêtes gRPC sont traduites en appels aux use cases
//! de la couche application via `SharedState`.

use tonic::{Request, Response, Status};
use tracing::info;
use uuid::Uuid;

use application::use_cases::create_operation::CreateOperationCommand;
use application::use_cases::list_operations::ListFilter;
use application::use_cases::search_chunks::ChunkSearchFilter;

use crate::errors::domain_error_to_status;
use crate::state::SharedState;

// Code généré par tonic-build à partir de proto/shinobi.proto.
pub mod proto {
    tonic::include_proto!("shinobi.ninpo");
}

use proto::shinobi_service_server::ShinobiService;
use proto::{
    ChunkListResponse, ChunkResponse, CreateOperationRequest, GetChunksRequest,
    GetOperationRequest, ListOperationsRequest, ListOperationsResponse, OperationResponse,
    PingRequest, PingResponse, SearchChunksRequest, SemanticChunkResponse,
    SemanticSearchRequest, SemanticSearchResponse,
};

/// Implémentation du service gRPC ShinobiService.
///
/// Reçoit `SharedState` contenant les use cases injectés.
pub struct ShinobiServiceImpl {
    state: SharedState,
}

impl ShinobiServiceImpl {
    /// Construit le service avec les use cases injectés.
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

/// Convertit une `Operation` domaine en réponse Protobuf.
fn operation_to_proto(op: domain::entities::operation::Operation) -> OperationResponse {
    OperationResponse {
        id: op.id.to_string(),
        author_id: op.author_id.to_string(),
        content_id: op.content_id.into_inner(),
        description: op.description,
        parent_ids: op.parent_ids.iter().map(|id| id.to_string()).collect(),
        created_at: op.created_at.timestamp(),
        ipfs_content_id: op.ipfs_content_id.map(|cid| cid.into_inner()),
    }
}

/// Convertit un `StoredChunk` domaine en réponse Protobuf.
fn chunk_to_proto(chunk: domain::ports::chunk_repository::StoredChunk) -> ChunkResponse {
    ChunkResponse {
        kind: chunk.kind,
        name: chunk.name.unwrap_or_default(),
        content: chunk.content,
        start_line: chunk.start_line as i32,
        end_line: chunk.end_line as i32,
        file_path: chunk.file_path,
        language: chunk.language,
    }
}

/// Convertit un `SimilarChunk` domaine en réponse Protobuf sémantique.
fn similar_chunk_to_proto(
    similar: domain::ports::chunk_repository::SimilarChunk,
) -> SemanticChunkResponse {
    SemanticChunkResponse {
        kind: similar.chunk.kind,
        name: similar.chunk.name.unwrap_or_default(),
        content: similar.chunk.content,
        start_line: similar.chunk.start_line as i32,
        end_line: similar.chunk.end_line as i32,
        file_path: similar.chunk.file_path,
        language: similar.chunk.language,
        similarity: similar.similarity,
    }
}

#[tonic::async_trait]
impl ShinobiService for ShinobiServiceImpl {
    /// Ping — Vérification de la disponibilité du serveur gRPC.
    async fn ping(
        &self,
        _request: Request<PingRequest>,
    ) -> Result<Response<PingResponse>, Status> {
        info!("Ninpo: Ping reçu");

        let response = PingResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            status: "operational".to_string(),
            uptime_seconds: 0, // TODO: calcul réel via Instant::now()
        };

        Ok(Response::new(response))
    }

    /// CreateOperation — Crée une opération VCS via le protocole Ninpo.
    async fn create_operation(
        &self,
        request: Request<CreateOperationRequest>,
    ) -> Result<Response<OperationResponse>, Status> {
        let req = request.into_inner();

        info!(
            author_id = %req.author_id,
            description = %req.description,
            "Ninpo: CreateOperation reçu"
        );

        let author_id = req
            .author_id
            .parse::<Uuid>()
            .map_err(|e| Status::invalid_argument(format!("author_id invalide: {e}")))?;

        let parent_ids: Vec<Uuid> = req
            .parent_ids
            .iter()
            .map(|id| {
                id.parse::<Uuid>()
                    .map_err(|e| Status::invalid_argument(format!("parent_id invalide: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let cmd = CreateOperationCommand {
            author_id,
            description: req.description,
            parent_ids,
            files: vec![],
        };

        let result = self
            .state
            .create_operation
            .execute(cmd)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(operation_to_proto(result.operation)))
    }

    /// GetOperation — Retrouver une opération par son ID.
    async fn get_operation(
        &self,
        request: Request<GetOperationRequest>,
    ) -> Result<Response<OperationResponse>, Status> {
        let req = request.into_inner();
        info!(id = %req.id, "Ninpo: GetOperation reçu");

        let id = req
            .id
            .parse::<Uuid>()
            .map_err(|e| Status::invalid_argument(format!("id invalide: {e}")))?;

        let operation = self
            .state
            .get_operation
            .execute(id)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(operation_to_proto(operation)))
    }

    /// ListOperations — Lister les opérations avec filtrage optionnel.
    async fn list_operations(
        &self,
        request: Request<ListOperationsRequest>,
    ) -> Result<Response<ListOperationsResponse>, Status> {
        let req = request.into_inner();
        info!(limit = req.limit, author_id = %req.author_id, "Ninpo: ListOperations reçu");

        let filter = if req.author_id.is_empty() {
            let limit = if req.limit > 0 { req.limit as usize } else { 50 };
            ListFilter::Recent { limit }
        } else {
            let author_id = req
                .author_id
                .parse::<Uuid>()
                .map_err(|e| Status::invalid_argument(format!("author_id invalide: {e}")))?;
            ListFilter::ByAuthor { author_id }
        };

        let operations = self
            .state
            .list_operations
            .execute(filter)
            .await
            .map_err(domain_error_to_status)?;

        let response = ListOperationsResponse {
            operations: operations.into_iter().map(operation_to_proto).collect(),
        };

        Ok(Response::new(response))
    }

    // ── Tensai: Mémoire IA ─────────────────────────

    /// GetChunksByOperation — Récupérer les fragments sémantiques d'une opération.
    async fn get_chunks_by_operation(
        &self,
        request: Request<GetChunksRequest>,
    ) -> Result<Response<ChunkListResponse>, Status> {
        let req = request.into_inner();

        info!(
            operation_id = %req.operation_id,
            file_path = %req.file_path,
            "Ninpo: GetChunksByOperation reçu (Tensai)"
        );

        let operation_id = req
            .operation_id
            .parse::<Uuid>()
            .map_err(|e| Status::invalid_argument(format!("operation_id invalide: {e}")))?;

        let filter = if req.file_path.is_empty() {
            ChunkSearchFilter::ByOperation { operation_id }
        } else {
            ChunkSearchFilter::ByFile {
                operation_id,
                file_path: req.file_path,
            }
        };

        let result = self
            .state
            .search_chunks
            .execute(filter)
            .await
            .map_err(domain_error_to_status)?;

        let response = ChunkListResponse {
            chunks: result.chunks.into_iter().map(chunk_to_proto).collect(),
            count: result.count as i32,
        };

        Ok(Response::new(response))
    }

    /// SearchChunksByName — Rechercher des symboles par nom.
    async fn search_chunks_by_name(
        &self,
        request: Request<SearchChunksRequest>,
    ) -> Result<Response<ChunkListResponse>, Status> {
        let req = request.into_inner();

        info!(
            name = %req.name,
            "Ninpo: SearchChunksByName reçu (Tensai)"
        );

        if req.name.is_empty() {
            return Err(Status::invalid_argument(
                "Le nom du symbole ne peut pas être vide",
            ));
        }

        let filter = ChunkSearchFilter::ByName { name: req.name };

        let result = self
            .state
            .search_chunks
            .execute(filter)
            .await
            .map_err(domain_error_to_status)?;

        let response = ChunkListResponse {
            chunks: result.chunks.into_iter().map(chunk_to_proto).collect(),
            count: result.count as i32,
        };

        Ok(Response::new(response))
    }

    // ── Phase 7A: Recherche Sémantique RAG ─────────

    /// SemanticSearch — Recherche par similarité sémantique (RAG vectoriel).
    async fn semantic_search(
        &self,
        request: Request<SemanticSearchRequest>,
    ) -> Result<Response<SemanticSearchResponse>, Status> {
        let req = request.into_inner();

        info!(
            query = %req.query,
            limit = req.limit,
            threshold = req.threshold,
            "Ninpo: SemanticSearch reçu (Tensai RAG)"
        );

        if req.query.is_empty() {
            return Err(Status::invalid_argument(
                "La requête de recherche ne peut pas être vide",
            ));
        }

        let limit = if req.limit > 0 { req.limit as usize } else { 10 };
        let threshold = if req.threshold > 0.0 { req.threshold } else { 0.5 };

        let result = self
            .state
            .search_chunks
            .execute_semantic(&req.query, limit, threshold)
            .await
            .map_err(domain_error_to_status)?;

        let response = SemanticSearchResponse {
            chunks: result
                .chunks
                .into_iter()
                .map(similar_chunk_to_proto)
                .collect(),
            count: result.count as i32,
        };

        Ok(Response::new(response))
    }
}
