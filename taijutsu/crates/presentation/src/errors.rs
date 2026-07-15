//! Mapping d'erreurs — Conversions DomainError vers HTTP et gRPC.
//!
//! Centralise la traduction des erreurs métier en réponses
//! compréhensibles par les consommateurs REST et gRPC.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use domain::errors::DomainError;

// ─── REST (Axum) ─────────────────────────────────

/// Wrapper pour convertir `DomainError` en réponse HTTP Axum.
pub struct AppError(pub DomainError);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            DomainError::NotFound { .. } => (StatusCode::NOT_FOUND, self.0.to_string()),
            DomainError::CommitNotFound { .. } => (StatusCode::NOT_FOUND, self.0.to_string()),
            DomainError::BusinessRule(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.0.to_string())
            }
            DomainError::Conflict(_) => (StatusCode::CONFLICT, self.0.to_string()),
            DomainError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, self.0.to_string()),
            DomainError::Forbidden(_) => (StatusCode::FORBIDDEN, self.0.to_string()),
            DomainError::Duplicate(_) => (StatusCode::CONFLICT, self.0.to_string()),
            DomainError::IsFile { .. } => {
                // Ne devrait pas arriver : le handler gère IsFile avant d'appeler From<DomainError>
                (StatusCode::UNPROCESSABLE_ENTITY, self.0.to_string())
            }
            DomainError::Persistence(_)
            | DomainError::VcsError(_)
            | DomainError::StorageError(_)
            | DomainError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Erreur interne du serveur".to_string(),
            ),
        };

        let body = serde_json::json!({
            "error": {
                "code": status.as_u16(),
                "message": message,
            }
        });

        (status, Json(body)).into_response()
    }
}

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        Self(err)
    }
}

// ─── gRPC (Tonic) ────────────────────────────────

/// Convertit une `DomainError` en `tonic::Status` pour les réponses gRPC.
///
/// Fonction libre (pas un `impl From`) pour respecter la règle d'orphelin
/// de Rust : on ne peut pas implémenter un trait étranger pour un type étranger.
pub fn domain_error_to_status(err: DomainError) -> tonic::Status {
    match &err {
        DomainError::NotFound { .. } => tonic::Status::not_found(err.to_string()),
        DomainError::CommitNotFound { .. } => tonic::Status::not_found(err.to_string()),
        DomainError::BusinessRule(_) => tonic::Status::invalid_argument(err.to_string()),
        DomainError::Conflict(_) => tonic::Status::already_exists(err.to_string()),
        DomainError::Unauthorized(_) => tonic::Status::unauthenticated(err.to_string()),
        DomainError::Forbidden(_) => tonic::Status::permission_denied(err.to_string()),
        DomainError::Duplicate(_) => tonic::Status::already_exists(err.to_string()),
        DomainError::IsFile { .. } => tonic::Status::invalid_argument(err.to_string()),
        DomainError::Persistence(_)
        | DomainError::VcsError(_)
        | DomainError::StorageError(_)
        | DomainError::Internal(_) => tonic::Status::internal("Erreur interne du serveur"),
    }
}
