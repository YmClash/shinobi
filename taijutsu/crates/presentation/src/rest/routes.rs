//! Routes REST — Routeurs Axum pour l'API HTTP.
//!
//! Point d'entrée HTTP du système SHINOBI.
//! Fournit les routes de santé et les futurs endpoints REST.

use axum::{Json, Router, routing::get};
use serde::Serialize;

/// Réponse du health check.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}

/// Construit le routeur Axum principal.
///
/// Les routes sont organisées par domaine fonctionnel.
/// Les use cases seront injectés via l'état Axum (State)
/// dans les phases suivantes.
pub fn create_router() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/status", get(status))
}

/// Health check — Vérification de la disponibilité du serveur.
///
/// `GET /health`
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "operational".to_string(),
        service: "taijutsu".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Status — Informations détaillées sur le serveur.
///
/// `GET /api/v1/status`
async fn status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "taijutsu",
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "vcs_engine": "jujutsu (stub)",
            "protocol": "ninpo (gRPC)",
            "persistence": "fūinjutsu (PostgreSQL + Redis)",
        },
        "status": "initializing"
    }))
}
