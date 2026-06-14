//! Routes REST — Routeurs Axum pour l'API HTTP.
//!
//! Point d'entrée HTTP du système SHINOBI.
//! Les use cases sont injectés via `SharedState` (Axum State extractor).

use axum::extract::{Path, Query, State};
use axum::{Json, Router, routing::get, routing::post};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use application::use_cases::create_operation::CreateOperationCommand;
use application::use_cases::list_operations::ListFilter;
use application::use_cases::search_chunks::ChunkSearchFilter;
use domain::entities::operation::Operation;
use domain::ports::chunk_repository::StoredChunk;

use crate::errors::AppError;
use crate::state::SharedState;

// ─── Types Request / Response ────────────────────

/// Réponse du health check.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}

/// Corps de la requête POST /api/v1/operations.
#[derive(Debug, Deserialize)]
pub struct CreateOperationBody {
    pub author_id: Uuid,
    pub description: String,
    #[serde(default)]
    pub parent_ids: Vec<Uuid>,
}

/// Paramètres de query pour GET /api/v1/operations.
#[derive(Debug, Deserialize)]
pub struct ListOperationsQuery {
    pub limit: Option<usize>,
    pub author_id: Option<Uuid>,
}

/// Paramètres de query pour GET /api/v1/operations/:id/chunks.
#[derive(Debug, Deserialize)]
pub struct ChunksQuery {
    /// Filtre optionnel par chemin de fichier (ex: "src/main.rs").
    pub file: Option<String>,
}

/// Paramètres de query pour GET /api/v1/chunks/search.
#[derive(Debug, Deserialize)]
pub struct SearchChunksQuery {
    /// Nom du symbole recherché.
    pub name: String,
}

/// Réponse JSON pour une opération.
#[derive(Debug, Serialize)]
pub struct OperationJson {
    pub id: Uuid,
    pub author_id: Uuid,
    pub content_id: String,
    /// CID IPFS distribué — null si non synchronisé (Genjutsu).
    pub ipfs_content_id: Option<String>,
    pub description: String,
    pub parent_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl From<Operation> for OperationJson {
    fn from(op: Operation) -> Self {
        Self {
            id: op.id,
            author_id: op.author_id,
            content_id: op.content_id.into_inner(),
            ipfs_content_id: op.ipfs_content_id.map(|cid| cid.into_inner()),
            description: op.description,
            parent_ids: op.parent_ids,
            created_at: op.created_at,
        }
    }
}

/// Réponse JSON pour un fragment sémantique.
#[derive(Debug, Serialize)]
pub struct ChunkJson {
    pub kind: String,
    pub name: Option<String>,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub file_path: String,
    pub language: String,
}

impl From<StoredChunk> for ChunkJson {
    fn from(chunk: StoredChunk) -> Self {
        Self {
            kind: chunk.kind,
            name: chunk.name,
            content: chunk.content,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            file_path: chunk.file_path,
            language: chunk.language,
        }
    }
}

// ─── Routeur ─────────────────────────────────────

/// Construit le routeur Axum principal avec les use cases injectés.
///
/// Intègre automatiquement le middleware Prometheus pour les métriques HTTP.
/// La route `/metrics` expose les métriques au format Prometheus scrape.
pub fn create_router(state: SharedState) -> Router {
    // ── Prometheus Middleware ───────────────────────
    let (prometheus_layer, metric_handle) = axum_prometheus::PrometheusMetricLayer::pair();

    Router::new()
        // Health & status (sans état)
        .route("/health", get(health_check))
        .route("/api/v1/status", get(status))
        // CRUD Operations
        .route(
            "/api/v1/operations",
            post(create_operation_handler).get(list_operations_handler),
        )
        .route("/api/v1/operations/{id}", get(get_operation_handler))
        // ── Tensai: Mémoire IA ─────────────────────
        .route(
            "/api/v1/operations/{id}/chunks",
            get(get_chunks_handler),
        )
        .route("/api/v1/chunks/search", get(search_chunks_handler))
        // ── Métriques Prometheus ────────────────────
        .route("/metrics", get(move || async move { metric_handle.render() }))
        .with_state(state)
        // Le layer doit être appliqué APRÈS .with_state() pour couvrir toutes les routes
        .layer(prometheus_layer)
}

// ─── Handlers ────────────────────────────────────

/// Health check — `GET /health`
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "operational".to_string(),
        service: "taijutsu".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Status — `GET /api/v1/status`
async fn status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "taijutsu",
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "vcs_engine": "jujutsu (ACL)",
            "protocol": "ninpo (gRPC)",
            "persistence": "fūinjutsu (PostgreSQL + Redis)",
            "tensai": "semantic chunking (Tree-sitter + ChunkRepository)",
        },
        "status": "operational"
    }))
}

/// Créer une opération — `POST /api/v1/operations`
async fn create_operation_handler(
    State(state): State<SharedState>,
    Json(body): Json<CreateOperationBody>,
) -> Result<(axum::http::StatusCode, Json<OperationJson>), AppError> {
    info!(
        author_id = %body.author_id,
        description = %body.description,
        "REST: CreateOperation reçu"
    );

    let cmd = CreateOperationCommand {
        author_id: body.author_id,
        description: body.description,
        parent_ids: body.parent_ids,
        files: vec![],
    };

    let result = state.create_operation.execute(cmd).await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(OperationJson::from(result.operation)),
    ))
}

/// Retrouver une opération — `GET /api/v1/operations/{id}`
async fn get_operation_handler(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OperationJson>, AppError> {
    info!(%id, "REST: GetOperation reçu");

    let operation = state.get_operation.execute(id).await?;

    Ok(Json(OperationJson::from(operation)))
}

/// Lister les opérations — `GET /api/v1/operations?limit=N&author_id=UUID`
async fn list_operations_handler(
    State(state): State<SharedState>,
    Query(params): Query<ListOperationsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(?params.limit, ?params.author_id, "REST: ListOperations reçu");

    let filter = if let Some(author_id) = params.author_id {
        ListFilter::ByAuthor { author_id }
    } else {
        ListFilter::Recent {
            limit: params.limit.unwrap_or(50),
        }
    };

    let operations = state.list_operations.execute(filter).await?;
    let operations_json: Vec<OperationJson> =
        operations.into_iter().map(OperationJson::from).collect();

    Ok(Json(serde_json::json!({
        "operations": operations_json,
        "count": operations_json.len(),
    })))
}

// ─── Handlers Tensai (Mémoire IA) ────────────────

/// Récupérer les chunks d'une opération — `GET /api/v1/operations/{id}/chunks`
///
/// Paramètres optionnels :
/// - `?file=src/main.rs` — filtre par fichier
async fn get_chunks_handler(
    State(state): State<SharedState>,
    Path(operation_id): Path<Uuid>,
    Query(params): Query<ChunksQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        %operation_id,
        file = ?params.file,
        "REST: GetChunks reçu (Tensai)"
    );

    let filter = match params.file {
        Some(file_path) => ChunkSearchFilter::ByFile {
            operation_id,
            file_path,
        },
        None => ChunkSearchFilter::ByOperation { operation_id },
    };

    let result = state.search_chunks.execute(filter).await?;
    let chunks_json: Vec<ChunkJson> = result.chunks.into_iter().map(ChunkJson::from).collect();

    Ok(Json(serde_json::json!({
        "operation_id": operation_id,
        "chunks": chunks_json,
        "count": result.count,
    })))
}

/// Rechercher des symboles par nom — `GET /api/v1/chunks/search?name=User`
async fn search_chunks_handler(
    State(state): State<SharedState>,
    Query(params): Query<SearchChunksQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        name = %params.name,
        "REST: SearchChunks reçu (Tensai)"
    );

    let filter = ChunkSearchFilter::ByName {
        name: params.name.clone(),
    };

    let result = state.search_chunks.execute(filter).await?;
    let chunks_json: Vec<ChunkJson> = result.chunks.into_iter().map(ChunkJson::from).collect();

    Ok(Json(serde_json::json!({
        "query": params.name,
        "chunks": chunks_json,
        "count": result.count,
    })))
}
