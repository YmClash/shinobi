//! Middleware Auth — Extracteurs Axum pour l'authentification (Phase 19A).
//!
//! Deux extracteurs :
//! - `AuthUser` : **obligatoire** — rejette 401 si absent ou invalide
//! - `MaybeAuth` : **optionnel** — retourne `None` si absent (repos publics)

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Json};

use domain::entities::session::AuthClaims;

// ── AuthUser (obligatoire) ───────────────────────────────────────────

/// Extracteur d'authentification obligatoire.
///
/// Extrait le JWT du header `Authorization: Bearer <token>`,
/// le vérifie et retourne les claims décodés.
///
/// Rejette avec 401 si :
/// - Le header est absent
/// - Le format n'est pas `Bearer <token>`
/// - Le JWT est invalide ou expiré
#[derive(Debug, Clone)]
pub struct AuthUser(pub AuthClaims);

/// Erreur d'authentification — 401 Unauthorized.
pub struct AuthError(String);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": {
                "code": 401,
                "message": self.0,
            }
        });
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    }
}

impl FromRequestParts<crate::state::SharedState> for AuthUser {
    type Rejection = AuthError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &crate::state::SharedState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let auth_service = state.auth_service.clone();
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        async move {
            let header = auth_header
                .ok_or_else(|| AuthError("Missing Authorization header".to_string()))?;

            let token = header
                .strip_prefix("Bearer ")
                .or_else(|| header.strip_prefix("bearer "))
                .ok_or_else(|| AuthError("Invalid Authorization format (expected Bearer)".to_string()))?;

            let claims = auth_service
                .verify_jwt(token)
                .map_err(|e| AuthError(e.to_string()))?;

            Ok(AuthUser(claims))
        }
    }
}

// ── MaybeAuth (optionnel) ────────────────────────────────────────────

/// Extracteur d'authentification optionnel.
///
/// Pour les routes accessibles publiquement mais qui peuvent enrichir
/// la réponse si l'utilisateur est authentifié (ex: repos publics).
///
/// Retourne `MaybeAuth(None)` si le header est absent ou invalide
/// (pas d'erreur, on continue en mode anonyme).
#[derive(Debug, Clone)]
pub struct MaybeAuth(pub Option<AuthClaims>);

impl FromRequestParts<crate::state::SharedState> for MaybeAuth {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &crate::state::SharedState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let auth_service = state.auth_service.clone();
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        async move {
            let claims = auth_header
                .and_then(|header| header.strip_prefix("Bearer ").map(|s| s.to_string()).or_else(|| header.strip_prefix("bearer ").map(|s| s.to_string())))
                .and_then(|token| auth_service.verify_jwt(&token).ok());

            Ok(MaybeAuth(claims))
        }
    }
}
