//! Routes REST - Pipelines CI/CD natifs (Phase 40 - Jutsu Runner) 🥷⚡
//!
//! ## Endpoints
//! | Methode | Route | Description |
//! |---|---|---|
//! | GET  | `/api/v1/repos/:owner/:repo/pipelines` | Lister les pipelines |
//! | GET  | `/api/v1/repos/:owner/:repo/pipelines/:id` | Detail + stages |
//! | POST | `/api/v1/repos/:owner/:repo/pipelines/trigger` | Declenchement manuel |
//! | GET  | `/api/v1/repos/:owner/:repo/pipelines/:id/stages` | Stages seuls (polling) |

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use domain::entities::pipeline::{Pipeline, PipelineStage, TriggerEvent};
use domain::errors::DomainError;

use crate::errors::AppError;
use crate::rest::auth_middleware::AuthUser;
use crate::state::SharedState;

// -- Response Types -------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PipelineStageResponse {
    pub id: Uuid,
    pub name: String,
    pub image: String,
    pub status: String,
    pub sort_order: i16,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i32>,
    pub exit_code: Option<i16>,
    /// Logs tronques (HEAD 100 + TAIL 200 lignes - 64 KB max)
    pub logs: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl PipelineStageResponse {
    pub fn from_stage(s: &PipelineStage) -> Self {
        Self {
            id: s.id,
            name: s.name.clone(),
            image: s.image.clone(),
            status: s.status.as_sql_str().to_string(),
            sort_order: s.sort_order,
            started_at: s.started_at.map(|t| t.to_rfc3339()),
            finished_at: s.finished_at.map(|t| t.to_rfc3339()),
            duration_ms: s.duration_ms,
            exit_code: s.exit_code,
            logs: s.logs.clone(),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PipelineResponse {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub commit_id: String,
    pub trigger_event: String,
    pub status: String,
    pub pipeline_name: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i32>,
    pub creator_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

impl PipelineResponse {
    pub fn from_pipeline(p: &Pipeline) -> Self {
        Self {
            id: p.id,
            repository_id: p.repository_id,
            commit_id: p.commit_id.clone(),
            trigger_event: p.trigger_event.as_sql_str().to_string(),
            status: p.status.as_sql_str().to_string(),
            pipeline_name: p.pipeline_name.clone(),
            started_at: p.started_at.map(|t| t.to_rfc3339()),
            finished_at: p.finished_at.map(|t| t.to_rfc3339()),
            duration_ms: p.duration_ms,
            creator_id: p.creator_id,
            created_at: p.created_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PipelineDetailResponse {
    #[serde(flatten)]
    pub pipeline: PipelineResponse,
    pub stages: Vec<PipelineStageResponse>,
}

/// Corps POST /trigger
#[derive(Debug, Deserialize)]
pub struct TriggerPipelineBody {
    /// SHA du commit a builder (obligatoire).
    pub commit_id: String,
    /// Reference / branche source (optionnel, pour affichage).
    pub ref_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PipelineListResponse {
    pub pipelines: Vec<PipelineResponse>,
    pub total: i64,
}

// -- Handlers -------------------------------------------------------------

/// GET /api/v1/repos/:owner/:repo/pipelines
pub(crate) async fn list_pipelines_handler(
    _auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Json<PipelineListResponse>, AppError> {
    let repository = state.resolve_repo.execute(&owner, &repo).await?;

    let pipelines = state
        .pipeline_repo
        .list_by_repo(&repository.id, 30)
        .await?;

    let total = pipelines.len() as i64;

    Ok(Json(PipelineListResponse {
        pipelines: pipelines.iter().map(PipelineResponse::from_pipeline).collect(),
        total,
    }))
}

/// GET /api/v1/repos/:owner/:repo/pipelines/:pipeline_id
pub(crate) async fn get_pipeline_handler(
    _auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, pipeline_id)): Path<(String, String, Uuid)>,
) -> Result<Json<PipelineDetailResponse>, AppError> {
    let repository = state.resolve_repo.execute(&owner, &repo).await?;

    let pipeline = state
        .pipeline_repo
        .find_by_id(&pipeline_id)
        .await?
        .ok_or(DomainError::NotFound { entity_type: "Pipeline", id: pipeline_id })?;

    // Verifier que le pipeline appartient bien a ce depot
    if pipeline.repository_id != repository.id {
        return Err(DomainError::NotFound { entity_type: "Pipeline", id: pipeline_id }.into());
    }

    let stages = state.pipeline_repo.list_stages(&pipeline_id).await?;

    Ok(Json(PipelineDetailResponse {
        pipeline: PipelineResponse::from_pipeline(&pipeline),
        stages: stages.iter().map(PipelineStageResponse::from_stage).collect(),
    }))
}

/// GET /api/v1/repos/:owner/:repo/pipelines/:pipeline_id/stages
/// Endpoint leger pour le polling live du frontend.
pub(crate) async fn list_pipeline_stages_handler(
    _auth: AuthUser,
    State(state): State<SharedState>,
    Path((_owner, _repo, pipeline_id)): Path<(String, String, Uuid)>,
) -> Result<Json<Vec<PipelineStageResponse>>, AppError> {
    let stages = state.pipeline_repo.list_stages(&pipeline_id).await?;
    Ok(Json(stages.iter().map(PipelineStageResponse::from_stage).collect()))
}

/// POST /api/v1/repos/:owner/:repo/pipelines/trigger
/// Declenchement manuel : cree un pipeline queued immediatement.
///
/// Graceful degradation : retourne 502 si Docker non disponible.
pub(crate) async fn trigger_pipeline_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    Json(body): Json<TriggerPipelineBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Docker non disponible -> 502 Bad Gateway (service externe manquant)
    if state.run_pipeline.is_none() {
        return Err(DomainError::External(
            "Jutsu Runner non disponible (Docker absent ou JUTSU_ENABLED=false)".to_string(),
        ).into());
    }

    if body.commit_id.is_empty() {
        return Err(DomainError::BusinessRule(
            "commit_id ne peut pas etre vide".to_string(),
        ).into());
    }

    let repository = state.resolve_repo.execute(&owner, &repo).await?;

    tracing::info!(
        actor_id = %auth.0.actor_id(),
        repo = %repo,
        commit_id = %body.commit_id,
        "🥷 Declenchement manuel de pipeline"
    );

    // Creer un pipeline queued pour que le frontend puisse tracker l'etat
    let pipeline = Pipeline::new(
        repository.id,
        body.commit_id.clone(),
        TriggerEvent::Manual,
        None,
        Some(auth.0.actor_id()),
    );
    state.pipeline_repo.create(&pipeline).await?;

    // NOTE: Le JutsuConsumer recevra l'evenement via Kafka (publie par le hook git_http)
    // Pour le trigger manuel, on insere directement en BDD et on retourne l'ID.
    // Le Makimono peut alors afficher la progression en temps reel via le polling.

    Ok(Json(serde_json::json!({
        "status": "queued",
        "pipeline_id": pipeline.id,
        "commit_id": body.commit_id,
        "message": "Pipeline en file d'attente.",
    })))
}
