//! Configuration centralisée du serveur Taijutsu.
//!
//! Charge les variables d'environnement via `dotenvy` (.env en dev,
//! variables système en production). Fail-fast si une variable
//! critique est absente.

use std::env;

/// Configuration complète du serveur Taijutsu.
#[derive(Debug, Clone)]
pub struct Config {
    /// URL de connexion PostgreSQL (Fūinjutsu).
    pub database_url: String,

    /// URL de connexion Redis (cache + verrous distribués).
    pub redis_url: String,

    /// Port du serveur REST (Axum). Défaut: 3000.
    pub rest_port: u16,

    /// Port du serveur gRPC / Ninpo (Tonic). Défaut: 50051.
    pub grpc_port: u16,

    /// Répertoire racine du workspace VCS (jj-lib).
    pub vcs_workspace_root: String,
}

impl Config {
    /// Charge la configuration depuis les variables d'environnement.
    ///
    /// # Panics
    /// Panique si `DATABASE_URL` ou `REDIS_URL` sont absentes
    /// (fail-fast au démarrage).
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL doit être défini (ex: postgres://user:pass@localhost/db)"),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
            rest_port: env::var("REST_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            grpc_port: env::var("GRPC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(50051),
            vcs_workspace_root: env::var("VCS_WORKSPACE_ROOT")
                .unwrap_or_else(|_| "./workspace".to_string()),
        }
    }
}
