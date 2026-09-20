//! Routes REST — Commit Status API (Phase 39 — Le Pont CI/CD) 🌉
//!
//! API pour la gestion des statuts de commit CI/CD.
//! Permet aux pipelines externes (Drone, Woodpecker, Jenkins) de reporter
//! le résultat de leurs builds directement dans Shinobi.
//!
//! ## Endpoints
//! | Méthode | Route | Description |
//! |---|---|---|
//! | POST | `/api/v1/repos/:owner/:repo/statuses/:commit_id` | Créer/mettre à jour un statut |
//! | GET  | `/api/v1/repos/:owner/:repo/statuses/:commit_id` | Lister les statuts d'un commit |
//! | GET  | `/api/v1/repos/:owner/:repo/statuses/:commit_id/combined` | Statut combiné |

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use domain::entities::commit_status::{CombinedStatus, CommitStatus};

use crate::errors::AppError;
use crate::rest::auth_middleware::AuthUser;
use crate::state::SharedState;

// ── Request/Response Types ────────────────────────────────────────────

/// Corps de la requête POST /statuses/:commit_id (création/mise à jour).
#[derive(Debug, Deserialize)]
pub struct CreateCommitStatusBody {
    /// État du build : "pending" | "success" | "failure" | "error".
    pub state: String,
    /// Identifiant du pipeline CI (ex: "drone/build", "woodpecker/test").
    pub context: String,
    /// Description courte (ex: "Build passed in 42s").
    pub description: Option<String>,
    /// URL vers le dashboard CI externe.
    pub target_url: Option<String>,
}

/// Réponse d'un statut de commit.
#[derive(Debug, Serialize)]
pub struct CommitStatusResponse {
    pub id: Uuid,
    pub commit_id: String,
    pub context: String,
    pub state: String,
    pub description: Option<String>,
    pub target_url: Option<String>,
    pub creator_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

impl CommitStatusResponse {
    /// Conversion depuis l'entité domaine.
    fn from_status(s: &CommitStatus) -> Self {
        Self {
            id: s.id,
            commit_id: s.commit_id.clone(),
            context: s.context.clone(),
            state: s.state.as_sql_str().to_string(),
            description: s.description.clone(),
            target_url: s.target_url.clone(),
            creator_id: s.creator_id,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

/// Réponse du statut combiné d'un commit.
#[derive(Debug, Serialize)]
pub struct CombinedStatusResponse {
    /// État combiné agrégé.
    pub state: String,
    /// Nombre total de statuts.
    pub total_count: usize,
    /// Détail de chaque statut individuel.
    pub statuses: Vec<CommitStatusResponse>,
}

impl CombinedStatusResponse {
    fn from_combined(c: &CombinedStatus) -> Self {
        Self {
            state: c.state.as_sql_str().to_string(),
            total_count: c.total_count,
            statuses: c.statuses.iter().map(CommitStatusResponse::from_status).collect(),
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────

/// POST /api/v1/repos/:owner/:repo/statuses/:commit_id — Créer/mettre à jour un statut
pub(crate) async fn create_commit_status_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, commit_id)): Path<(String, String, String)>,
    Json(body): Json<CreateCommitStatusBody>,
) -> Result<Json<CommitStatusResponse>, AppError> {
    let status = state
        .manage_commit_statuses
        .create_or_update_status(
            auth.0.actor_id(),
            &owner,
            &repo,
            &commit_id,
            &body.state,
            &body.context,
            body.description,
            body.target_url,
        )
        .await?;

    Ok(Json(CommitStatusResponse::from_status(&status)))
}

/// GET /api/v1/repos/:owner/:repo/statuses/:commit_id — Lister les statuts d'un commit
pub(crate) async fn list_commit_statuses_handler(
    _auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, commit_id)): Path<(String, String, String)>,
) -> Result<Json<Vec<CommitStatusResponse>>, AppError> {
    let statuses = state
        .manage_commit_statuses
        .list_statuses(&owner, &repo, &commit_id)
        .await?;

    let response: Vec<CommitStatusResponse> = statuses
        .iter()
        .map(CommitStatusResponse::from_status)
        .collect();

    Ok(Json(response))
}

/// GET /api/v1/repos/:owner/:repo/statuses/:commit_id/combined — Statut combiné
pub(crate) async fn combined_commit_status_handler(
    _auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, commit_id)): Path<(String, String, String)>,
) -> Result<Json<CombinedStatusResponse>, AppError> {
    let combined = state
        .manage_commit_statuses
        .combined_status(&owner, &repo, &commit_id)
        .await?;

    Ok(Json(CombinedStatusResponse::from_combined(&combined)))
}
