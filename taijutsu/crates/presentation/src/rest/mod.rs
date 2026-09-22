//! Adaptateur REST — Serveur HTTP via Axum.

pub mod auth_middleware;
pub mod auth_routes;
pub mod federation;
pub mod git_http;
pub mod routes;
pub mod webhook_routes;
pub mod commit_status_routes;
pub mod pipeline_routes;
