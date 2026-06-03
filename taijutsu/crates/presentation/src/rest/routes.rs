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
use domain::entities::operation::Operation;

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

/// Réponse JSON pour une opération.
#[derive(Debug, Serialize)]
pub struct OperationJson {
    pub id: Uuid,
    pub author_id: Uuid,
    pub content_id: String,
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
            description: op.description,
            parent_ids: op.parent_ids,
            created_at: op.created_at,
        }
    }
}

// ─── Routeur ─────────────────────────────────────

/// Construit le routeur Axum principal avec les use cases injectés.
pub fn create_router(state: SharedState) -> Router {
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
        .with_state(state)
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
