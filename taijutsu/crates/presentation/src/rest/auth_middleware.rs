//! Middleware Auth — Extracteurs Axum pour l'authentification (Phase 19A).
//!
//! Deux extracteurs + un middleware layer :
//! - `AuthUser` : **obligatoire** — rejette 401 si absent ou invalide
//! - `MaybeAuth` : **optionnel** — retourne `None` si absent (repos publics)
//! - `require_auth_layer` : **middleware global** — rejette 401 en amont (Bouclier Global)

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Json};
use base64::Engine as _;

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
        let actor_repo = state.actor_repo.clone();
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        async move {
            let header = auth_header
                .ok_or_else(|| AuthError("Missing Authorization header".to_string()))?;

            // Try Bearer JWT first
            if let Some(token) = header
                .strip_prefix("Bearer ")
                .or_else(|| header.strip_prefix("bearer "))
            {
                let claims = auth_service
                    .verify_jwt(token)
                    .map_err(|e| AuthError(e.to_string()))?;
                return Ok(AuthUser(claims));
            }

            // Try Basic Auth (PAT) — Phase 28B: ANBU CLI support
            if let Some(encoded) = header
                .strip_prefix("Basic ")
                .or_else(|| header.strip_prefix("basic "))
            {
                let decoded = String::from_utf8(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_| AuthError("Invalid Basic Auth encoding".to_string()))?,
                )
                .map_err(|_| AuthError("Invalid Basic Auth UTF-8".to_string()))?;

                let (_username, pat) = decoded
                    .split_once(':')
                    .ok_or_else(|| AuthError("Invalid Basic Auth format".to_string()))?;

                // Hash the PAT and look up the actor
                let pat_hash = auth_service.hash_pat_for_lookup(pat);
                let actor = actor_repo
                    .find_actor_by_credential_hash(&pat_hash, "api_key")
                    .await
                    .map_err(|e| AuthError(format!("Auth lookup error: {e}")))?
                    .ok_or_else(|| AuthError("Invalid Personal Access Token".to_string()))?;

                // Generate ephemeral claims (same as a JWT would contain)
                let claims = domain::entities::session::AuthClaims::new(
                    actor.id,
                    actor.handle.clone(),
                    actor.actor_type,
                    3600, // 1 hour ephemeral validity
                );
                return Ok(AuthUser(claims));
            }

            Err(AuthError("Invalid Authorization format (expected Bearer or Basic)".to_string()))
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

// ── Bouclier Global — require_auth_layer (Phase 27-pre) ──────────────

/// Middleware Axum — Bouclier Global.
///
/// Rejette automatiquement toute requête sans JWT valide (401 Unauthorized).
/// Appliqué comme `.layer()` sur le routeur privé et comme `.route_layer()`
/// sur les méthodes protégées des routes mixtes.
///
/// ## Defense in Depth
/// Ce middleware est le **filet de sécurité global**. Même si un handler
/// oublie l'extracteur `AuthUser`, la requête est déjà rejetée en amont.
/// Les handlers continuent d'utiliser `AuthUser` pour extraire les claims.
pub async fn require_auth_layer(
    axum::extract::State(state): axum::extract::State<crate::state::SharedState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, Response> {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let header = auth_header.ok_or_else(|| {
        let body = serde_json::json!({
            "error": {
                "code": 401,
                "message": "Authentification requise",
            }
        });
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    })?;

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or_else(|| {
            let body = serde_json::json!({
                "error": {
                    "code": 401,
                    "message": "Format Authorization invalide (expected Bearer)",
                }
            });
            (StatusCode::UNAUTHORIZED, Json(body)).into_response()
        })?;

    // Vérifier le JWT
    state.auth_service.verify_jwt(token).map_err(|e| {
        let body = serde_json::json!({
            "error": {
                "code": 401,
                "message": e.to_string(),
            }
        });
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    })?;

    // JWT valide → laisser passer (le handler extraira les claims via AuthUser)
    Ok(next.run(request).await)
}

