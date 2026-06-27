//! Routes REST — Routeurs Axum pour l'API HTTP.
//!
//! Point d'entrée HTTP du système SHINOBI.
//! Les use cases sont injectés via `SharedState` (Axum State extractor).

use axum::extract::{Path, Query, State};
use axum::{Json, Router, routing::get, routing::post};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use application::use_cases::create_operation::CreateOperationCommand;
use application::use_cases::list_operations::ListFilter;
use application::use_cases::search_chunks::ChunkSearchFilter;
use domain::entities::operation::Operation;
use domain::ports::chunk_repository::{SimilarChunk, StoredChunk};
use domain::ports::review_repository::OperationReview;

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
    /// Fichiers à inclure dans l'opération (optionnel).
    #[serde(default)]
    pub files: Vec<FileEntryBody>,
}

/// Entrée de fichier dans le body de la requête.
#[derive(Debug, Deserialize)]
pub struct FileEntryBody {
    /// Chemin du fichier (ex: "src/main.rs").
    pub path: String,
    /// Contenu encodé en base64 (RFC 4648).
    pub content_b64: String,
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

/// Body de la requête POST /api/v1/chunks/semantic-search.
#[derive(Debug, Deserialize)]
pub struct SemanticSearchBody {
    /// Requête en langage naturel.
    pub query: String,
    /// Nombre maximum de résultats (défaut: 10).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Score minimum de similarité (0.0 à 1.0, défaut: 0.5).
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

fn default_limit() -> usize { 10 }
fn default_threshold() -> f32 { 0.5 }

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

/// Réponse JSON pour un fragment sémantique avec score de similarité (Phase 7A).
#[derive(Debug, Serialize)]
pub struct SemanticChunkJson {
    pub kind: String,
    pub name: Option<String>,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub file_path: String,
    pub language: String,
    pub similarity: f32,
}

impl From<SimilarChunk> for SemanticChunkJson {
    fn from(similar: SimilarChunk) -> Self {
        Self {
            kind: similar.chunk.kind,
            name: similar.chunk.name,
            content: similar.chunk.content,
            start_line: similar.chunk.start_line,
            end_line: similar.chunk.end_line,
            file_path: similar.chunk.file_path,
            language: similar.chunk.language,
            similarity: similar.similarity,
        }
    }
}

/// Réponse JSON pour une code review IA (Phase 9 — Oracle).
#[derive(Debug, Serialize)]
pub struct ReviewJson {
    pub id: Uuid,
    pub reviewer: String,
    pub model: String,
    pub summary: String,
    pub content: String,
    pub score: Option<f32>,
    pub duration_ms: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<OperationReview> for ReviewJson {
    fn from(review: OperationReview) -> Self {
        Self {
            id: review.id,
            reviewer: review.reviewer,
            model: review.model,
            summary: review.summary,
            content: review.content,
            score: review.score,
            duration_ms: review.duration_ms,
            created_at: review.created_at,
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
        // ── VCS Diff ───────────────────────────────
        .route("/api/v1/operations/{id}/diff", get(get_operation_diff_handler))
        // ── Oracle: Code Reviews IA ────────────────
        .route("/api/v1/operations/{id}/reviews", get(get_reviews_handler))
        // ── Oracle: Sparkline Scores (Phase 9.2) ──
        .route("/api/v1/reviews/scores", get(get_score_history_handler))
        // ── IPFS Content Explorer ──────────────────
        .route("/api/v1/operations/{id}/ipfs", get(get_ipfs_content_handler))
        // ── Tensai: Mémoire IA ─────────────────────
        .route(
            "/api/v1/operations/{id}/chunks",
            get(get_chunks_handler),
        )
        .route("/api/v1/chunks/search", get(search_chunks_handler))
        // ── Tensai: Recherche Sémantique RAG (Phase 7A) ──
        .route(
            "/api/v1/chunks/semantic-search",
            post(semantic_search_handler),
        )
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

    // Décoder les fichiers base64 → bytes bruts.
    let files: Vec<(String, Vec<u8>)> = body
        .files
        .iter()
        .filter_map(|f| {
            match decode_base64(&f.content_b64) {
                Ok(bytes) => Some((f.path.clone(), bytes)),
                Err(e) => {
                    warn!(path = %f.path, error = %e, "⚠️ Décodage base64 échoué — fichier ignoré");
                    None
                }
            }
        })
        .collect();

    let cmd = CreateOperationCommand {
        author_id: body.author_id,
        description: body.description,
        parent_ids: body.parent_ids,
        files,
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

// ─── Handler Diff VCS ─────────────────────────────

/// Récupérer les fichiers modifiés par une opération — `GET /api/v1/operations/{id}/diff`
async fn get_operation_diff_handler(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(%id, "REST: GetOperationDiff reçu");

    let result = state.get_operation_diff.execute(id).await?;

    Ok(Json(serde_json::json!({
        "operation_id": result.operation_id,
        "content_id": result.content_id,
        "changed_files": result.changed_files,
        "count": result.changed_files.len(),
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

// ─── Handler Recherche Sémantique (Phase 7A) ─────────

/// Recherche sémantique RAG — `POST /api/v1/chunks/semantic-search`
///
/// Transforme la requête en langage naturel en embedding vectoriel,
/// puis exécute une recherche par similarité cosinus via pgvector.
async fn semantic_search_handler(
    State(state): State<SharedState>,
    Json(body): Json<SemanticSearchBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        query = %body.query,
        limit = body.limit,
        threshold = body.threshold,
        "REST: SemanticSearch reçu (Tensai RAG)"
    );

    let result = state
        .search_chunks
        .execute_semantic(&body.query, body.limit, body.threshold)
        .await?;

    let chunks_json: Vec<SemanticChunkJson> = result
        .chunks
        .into_iter()
        .map(SemanticChunkJson::from)
        .collect();

    Ok(Json(serde_json::json!({
        "query": body.query,
        "chunks": chunks_json,
        "count": result.count,
    })))
}

// ── Helpers ───────────────────────────────────────────────────────

/// Décode une chaîne base64 (RFC 4648) en bytes bruts.
///
/// Implémentation inline — cohérente avec l'encodeur dans `create_operation.rs`.
/// Évite l'ajout du crate `base64` pour une seule fonction.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    const DECODE_TABLE: [u8; 128] = {
        let mut table = [255u8; 128];
        let mut i = 0u8;
        while i < 26 {
            table[(b'A' + i) as usize] = i;
            table[(b'a' + i) as usize] = i + 26;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            table[(b'0' + d) as usize] = d + 52;
            d += 1;
        }
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
        table
    };

    let input = input.trim();
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();

    for chunk in bytes.chunks(4) {
        let mut buf = [0u32; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b >= 128 || DECODE_TABLE[b as usize] == 255 {
                return Err(format!("Invalid base64 character: {}", b as char));
            }
            buf[i] = DECODE_TABLE[b as usize] as u32;
        }
        let triple = (buf[0] << 18) | (buf[1] << 12) | (buf[2] << 6) | buf[3];
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk.len() > 2 {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 {
            result.push((triple & 0xFF) as u8);
        }
    }

    Ok(result)
}

// ─── Handler IPFS Content Explorer ────────────────

/// Récupérer le contenu IPFS d'une opération — `GET /api/v1/operations/{id}/ipfs`
async fn get_ipfs_content_handler(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(%id, "REST: GetIpfsContent reçu");

    let result = state.get_ipfs_content.execute(id).await?;

    Ok(Json(serde_json::json!({
        "operation_id": result.operation_id,
        "ipfs_cid": result.ipfs_cid,
        "blob_size": result.blob_size,
        "files": result.files,
        "count": result.files.len(),
    })))
}

// ─── Handler Oracle Reviews (Phase 9) ─────────────────

/// Récupérer les code reviews d'une opération — `GET /api/v1/operations/{id}/reviews`
async fn get_reviews_handler(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(%id, "REST: GetReviews reçu (Oracle)");

    let result = state.get_reviews.execute(id).await?;
    let reviews_json: Vec<ReviewJson> = result
        .reviews
        .into_iter()
        .map(ReviewJson::from)
        .collect();

    Ok(Json(serde_json::json!({
        "operation_id": id,
        "reviews": reviews_json,
        "count": result.count,
    })))
}

// ─── Handler Score History (Phase 9.2) ───────────────

/// Paramètres de query pour GET /api/v1/reviews/scores.
#[derive(Debug, Deserialize)]
pub struct ScoreHistoryQuery {
    /// Nombre de scores à récupérer (défaut: 10).
    #[serde(default = "default_score_limit")]
    pub limit: usize,
}

fn default_score_limit() -> usize { 10 }

/// Récupérer l'historique des scores Oracle — `GET /api/v1/reviews/scores?limit=10`
async fn get_score_history_handler(
    State(state): State<SharedState>,
    Query(params): Query<ScoreHistoryQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(limit = params.limit, "REST: GetScoreHistory reçu (Sparkline)");

    let result = state.get_score_history.execute(params.limit).await?;

    Ok(Json(serde_json::json!({
        "scores": result.scores,
        "count": result.count,
        "average": result.average,
        "trend": result.trend,
    })))
}
