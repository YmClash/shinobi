//! Routes REST — Réglages du dépôt (Phase 42-pre-Fix) 🔒
//!
//! | Méthode | Route | Description |
//! |---|---|---|
//! | PATCH | `/api/v1/repos/:owner/:repo/settings` | Politique de confidentialité IA (owner) |

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use application::use_cases::update_repo_settings::UpdateRepoSettingsUseCase;

use crate::errors::AppError;
use crate::rest::auth_middleware::AuthUser;
use crate::state::SharedState;

/// Corps PATCH /settings — tous les champs sont optionnels.
#[derive(Debug, Deserialize)]
pub struct UpdateRepoSettingsBody {
    /// Autorise l'envoi du code/logs à un fournisseur LLM externe.
    pub allow_external_llm: Option<bool>,
}

/// PATCH /api/v1/repos/:owner/:repo/settings
///
/// 🔒 Réservé au **owner** du dépôt (403 sinon).
pub(crate) async fn update_repo_settings_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    Json(body): Json<UpdateRepoSettingsBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repository = state.resolve_repo.execute(&owner, &repo).await?;
    let uc = UpdateRepoSettingsUseCase::new(state.repo_repo.clone());

    let mut allow_external_llm = repository.allow_external_llm;
    if let Some(allow) = body.allow_external_llm {
        uc.set_allow_external_llm(&auth.0.actor_id(), &repository.id, allow)
            .await?;
        allow_external_llm = allow;
    }

    Ok(Json(serde_json::json!({
        "repository_id": repository.id,
        "allow_external_llm": allow_external_llm,
    })))
}
