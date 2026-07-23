//! Routes d'authentification — Register, Login, Me, Tokens (Phase 19A).
//!
//! Routes publiques (register, login) et protégées (me, tokens).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use application::use_cases::register_actor::RegisterCommand;
use application::use_cases::login_actor::LoginCommand;
use application::use_cases::create_pat::CreatePatCommand;

use crate::errors::AppError;
use crate::rest::auth_middleware::AuthUser;
use crate::state::SharedState;

// ── DTOs ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub handle: String,
    pub display_name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePatRequest {
    pub label: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────

/// POST /api/v1/auth/register — Inscription ouverte.
pub async fn register_handler(
    State(state): State<SharedState>,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .register_actor
        .execute(RegisterCommand {
            handle: body.handle,
            display_name: body.display_name,
            email: body.email,
            password: body.password,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "token": result.token,
            "actor": {
                "id": result.actor.id,
                "handle": result.actor.handle,
                "display_name": result.actor.display_name,
                "actor_type": result.actor.actor_type.as_sql_str(),
                "email": result.actor.email,
                "avatar_url": result.actor.avatar_url,
                "created_at": result.actor.created_at,
            }
        })),
    ))
}

/// POST /api/v1/auth/login — Connexion.
pub async fn login_handler(
    State(state): State<SharedState>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .login_actor
        .execute(LoginCommand {
            email: body.email,
            password: body.password,
        })
        .await?;

    Ok(Json(serde_json::json!({
        "token": result.token,
        "actor": {
            "id": result.actor.id,
            "handle": result.actor.handle,
            "display_name": result.actor.display_name,
            "actor_type": result.actor.actor_type.as_sql_str(),
            "email": result.actor.email,
            "avatar_url": result.actor.avatar_url,
            "created_at": result.actor.created_at,
        }
    })))
}

/// GET /api/v1/auth/me — Profil de l'acteur authentifié.
pub async fn me_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let actor = state
        .actor_repo
        .find_by_id(&auth.0.actor_id())
        .await?
        .ok_or_else(|| domain::errors::DomainError::NotFound {
            entity_type: "Actor",
            id: auth.0.actor_id(),
        })?;

    Ok(Json(serde_json::json!({
        "id": actor.id,
        "handle": actor.handle,
        "display_name": actor.display_name,
        "actor_type": actor.actor_type.as_sql_str(),
        "email": actor.email,
        "avatar_url": actor.avatar_url,
        "bio": actor.bio,
        "github_id": actor.github_id,
        "created_at": actor.created_at,
    })))
}

/// POST /api/v1/auth/tokens — Créer un Personal Access Token.
pub async fn create_pat_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(body): Json<CreatePatRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .create_pat
        .execute(CreatePatCommand {
            actor_id: auth.0.actor_id(),
            label: body.label,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "token": result.raw_token,
            "label": result.label,
            "warning": "Ce token ne sera plus affiché. Copiez-le maintenant !",
        })),
    ))
}

/// GET /api/v1/auth/tokens — Lister les PAT de l'acteur authentifié.
pub async fn list_pats_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let pats = state
        .create_pat
        .list(&auth.0.actor_id())
        .await?;

    Ok(Json(serde_json::json!({
        "tokens": pats,
        "count": pats.len(),
    })))
}

// ── GitHub OAuth (Phase 20) ──────────────────────────────────────────

/// GET /api/v1/auth/github — Génère l'URL d'autorisation GitHub.
pub async fn github_auth_url_handler(
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, AppError> {
    let oauth = state.oauth_github.as_ref().ok_or_else(|| {
        AppError::from(domain::errors::DomainError::BusinessRule(
            "GitHub OAuth n'est pas configuré sur ce serveur".to_string(),
        ))
    })?;

    let redirect_uri = format!(
        "{}/auth/github/callback",
        state.frontend_url.as_deref().unwrap_or("http://localhost:3001")
    );

    let (url, _state_token) = oauth.generate_auth_url(&redirect_uri).await?;

    Ok(Json(serde_json::json!({
        "url": url,
    })))
}

/// POST /api/v1/auth/github/callback — Échange le code OAuth → JWT SHINOBI.
#[derive(Debug, Deserialize)]
pub struct GitHubCallbackRequest {
    pub code: String,
    pub state: String,
}

pub async fn github_callback_handler(
    State(state): State<SharedState>,
    Json(body): Json<GitHubCallbackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let oauth = state.oauth_github.as_ref().ok_or_else(|| {
        AppError::from(domain::errors::DomainError::BusinessRule(
            "GitHub OAuth n'est pas configuré sur ce serveur".to_string(),
        ))
    })?;

    use application::use_cases::oauth_github::OAuthGitHubCommand;

    let result = oauth
        .execute(OAuthGitHubCommand {
            code: body.code,
            state: body.state,
        })
        .await?;

    let status = if result.is_new_account {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(serde_json::json!({
            "token": result.token,
            "actor": {
                "id": result.actor.id,
                "handle": result.actor.handle,
                "display_name": result.actor.display_name,
                "actor_type": result.actor.actor_type.as_sql_str(),
                "email": result.actor.email,
                "avatar_url": result.actor.avatar_url,
                "created_at": result.actor.created_at,
            },
            "is_new_account": result.is_new_account,
        })),
    ))
}

// ── Phase 25 — Service Accounts (L'Acte de Naissance) ────────────────

#[derive(Debug, Deserialize)]
pub struct CreateServiceAccountRequest {
    pub handle: String,
    pub display_name: String,
    pub label: Option<String>,
}

/// POST /api/v1/auth/service-accounts — Créer un Service Account.
///
/// Retourne le bot créé et le PAT en clair (unique affichage).
pub async fn create_service_account_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(body): Json<CreateServiceAccountRequest>,
) -> Result<impl IntoResponse, AppError> {
    use application::use_cases::create_service_account::CreateServiceAccountCommand;

    let result = state
        .create_service_account
        .execute(CreateServiceAccountCommand {
            parent_actor_id: auth.0.actor_id(),
            handle: body.handle,
            display_name: body.display_name,
            label: body.label,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "actor": {
                "id": result.actor.id,
                "handle": result.actor.handle,
                "display_name": result.actor.display_name,
                "actor_type": result.actor.actor_type.as_sql_str(),
                "parent_id": result.actor.parent_id,
                "created_at": result.actor.created_at,
            },
            "token": result.raw_token,
            "warning": "Ce token ne sera plus affiché. Copiez-le maintenant !",
        })),
    ))
}

/// GET /api/v1/auth/service-accounts — Lister mes Service Accounts.
pub async fn list_service_accounts_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let bots = state
        .create_service_account
        .list(&auth.0.actor_id())
        .await?;

    let bots_json: Vec<serde_json::Value> = bots
        .iter()
        .map(|bot| {
            serde_json::json!({
                "id": bot.id,
                "handle": bot.handle,
                "display_name": bot.display_name,
                "actor_type": bot.actor_type.as_sql_str(),
                "parent_id": bot.parent_id,
                "avatar_url": bot.avatar_url,
                "created_at": bot.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "service_accounts": bots_json,
        "count": bots.len(),
    })))
}

/// DELETE /api/v1/auth/service-accounts/{id} — Supprimer un Service Account.
pub async fn delete_service_account_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
    axum::extract::Path(bot_id): axum::extract::Path<uuid::Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let deleted = state
        .create_service_account
        .delete(&auth.0.actor_id(), &bot_id)
        .await?;

    if deleted {
        Ok(Json(serde_json::json!({
            "deleted": true,
            "id": bot_id,
        })))
    } else {
        Err(AppError::from(domain::errors::DomainError::NotFound {
            entity_type: "ServiceAccount",
            id: bot_id,
        }))
    }
}

