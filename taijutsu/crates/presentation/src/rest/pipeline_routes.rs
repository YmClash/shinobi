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
use domain::entities::kage_bunshin::{HealAttempt, PatchHunk};
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

    // Publier sur Kafka pour que le JutsuConsumer lance l'exécution Docker.
    // Sans cela, le pipeline resterait stuck en "queued" indéfiniment.
    if let Some(publisher) = &state.event_publisher {
        publisher.publish_pipeline_requested(
            repository.id,
            &body.commit_id,
            "manual",
            Some(pipeline.id),
        ).await?;
    }

    Ok(Json(serde_json::json!({
        "status": "queued",
        "pipeline_id": pipeline.id,
        "commit_id": body.commit_id,
        "message": "Pipeline en file d'attente.",
    })))
}

// -- Phase 41-B : Kage Bunshin REST endpoint --------------------------------

#[derive(Debug, Serialize)]
pub struct PatchHunkResponse {
    pub path: String,
    pub search: String,
    pub replace: String,
}

impl PatchHunkResponse {
    fn from_hunk(h: &PatchHunk) -> Self {
        Self {
            path: h.path.clone(),
            search: h.search.clone(),
            replace: h.replace.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HealAttemptResponse {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub stage_name: String,
    pub diagnosis: String,
    pub patch_summary: Option<String>,
    pub hunks: Vec<PatchHunkResponse>,
    pub status: String,
    pub shadow_branch: Option<String>,
    pub mr_id: Option<Uuid>,
    pub retry_logs: Option<String>,
    pub retry_exit_code: Option<i16>,
    pub llm_model: Option<String>,
    pub llm_duration_ms: Option<i32>,
    pub confidence: Option<f32>,
    pub created_at: String,
    pub updated_at: String,
}

impl HealAttemptResponse {
    pub fn from_attempt(a: &HealAttempt) -> Self {
        Self {
            id: a.id,
            pipeline_id: a.pipeline_id,
            stage_name: a.stage_name.clone(),
            diagnosis: a.diagnosis.clone(),
            patch_summary: a.patch_summary.clone(),
            hunks: a.hunks.iter().map(PatchHunkResponse::from_hunk).collect(),
            status: a.status.as_sql_str().to_string(),
            shadow_branch: a.shadow_branch.clone(),
            mr_id: a.mr_id,
            retry_logs: a.retry_logs.clone(),
            retry_exit_code: a.retry_exit_code,
            llm_model: a.llm_model.clone(),
            llm_duration_ms: a.llm_duration_ms,
            confidence: a.confidence,
            created_at: a.created_at.to_rfc3339(),
            updated_at: a.updated_at.to_rfc3339(),
        }
    }
}

/// GET /api/v1/repos/:owner/:repo/pipelines/:pipeline_id/heals
/// Liste les tentatives de guérison (heal_attempts) d'un pipeline.
///
/// Phase 41-B : Endpoint dédié pour le panneau Kage Bunshin dans Makimono.
/// Les résultats sont ordonnés par `created_at DESC` (Tweak "Tri Temporel")
/// pour que le frontend prenne toujours la tentative la plus récente par stage.
pub(crate) async fn list_heals_handler(
    _auth: AuthUser,
    State(state): State<SharedState>,
    Path((_owner, _repo, pipeline_id)): Path<(String, String, Uuid)>,
) -> Result<Json<Vec<HealAttemptResponse>>, AppError> {
    let attempts = state
        .pipeline_repo
        .list_heal_attempts(&pipeline_id)
        .await?;
    Ok(Json(
        attempts.iter().map(HealAttemptResponse::from_attempt).collect(),
    ))
}

