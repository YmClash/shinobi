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

    /// Brokers Kafka (Nen). Défaut: "localhost:9092".
    pub kafka_brokers: String,

    /// Topic Kafka pour les événements VCS. Défaut: "shinobi.vcs.operations".
    pub kafka_topic: String,

    /// Topic Kafka pour les événements analysis-complete (Phase 7B).
    /// Défaut: "shinobi.tensai.analysis-complete".
    /// Topic dédié, séparé du topic VCS pour éviter la boucle infinie.
    pub kafka_analysis_topic: String,

    /// URL de l'API RPC IPFS / Kubo (Genjutsu). Défaut: "http://127.0.0.1:5001".
    pub ipfs_api_url: String,

    /// Consumer group Kafka pour l'agent Tensai. Défaut: "shinobi-tensai-analyzer".
    pub kafka_consumer_group: String,

    /// Activer/désactiver le consumer Tensai. Défaut: true.
    pub tensai_consumer_enabled: bool,

    /// Activer/désactiver l'embedding vectoriel (Phase 7A). Défaut: true.
    pub embedding_enabled: bool,

    /// Nombre de dimensions d'embedding (Matryoshka). Défaut: 256.
    pub embedding_dimensions: usize,

    /// URL du serveur Ollama Docker (LLM local). Défaut: "http://localhost:11435".
    pub ollama_url: String,

    /// Modèle Ollama à utiliser. Défaut: "granite3.1-dense:2b".
    pub ollama_model: String,

    /// Activer/désactiver le consumer Oracle. Défaut: true.
    pub oracle_consumer_enabled: bool,

    /// Consumer group Kafka pour l'agent Oracle. Défaut: "shinobi-oracle-reviewer".
    pub oracle_consumer_group: String,
}

impl Config {
    /// Charge la configuration depuis les variables d'environnement.
    ///
    /// # Panics
    /// Panique si `DATABASE_URL` est absente (fail-fast au démarrage).
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
            kafka_brokers: env::var("KAFKA_BROKERS")
                .unwrap_or_else(|_| "localhost:9092".to_string()),
            kafka_topic: env::var("KAFKA_TOPIC")
                .unwrap_or_else(|_| "shinobi.vcs.operations".to_string()),
            kafka_analysis_topic: env::var("KAFKA_ANALYSIS_TOPIC")
                .unwrap_or_else(|_| "shinobi.tensai.analysis-complete".to_string()),
            ipfs_api_url: env::var("IPFS_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:5001".to_string()),
            kafka_consumer_group: env::var("KAFKA_CONSUMER_GROUP")
                .unwrap_or_else(|_| "shinobi-tensai-analyzer".to_string()),
            tensai_consumer_enabled: env::var("TENSAI_CONSUMER_ENABLED")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            embedding_enabled: env::var("EMBEDDING_ENABLED")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            embedding_dimensions: env::var("EMBEDDING_DIMENSIONS")
                .ok()
                .and_then(|d| d.parse().ok())
                .unwrap_or(256),
            ollama_url: env::var("OLLAMA_URL")
                .unwrap_or_else(|_| "http://localhost:11435".to_string()),
            ollama_model: env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| "granite3-dense:2b".to_string()),
            oracle_consumer_enabled: env::var("ORACLE_CONSUMER_ENABLED")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            oracle_consumer_group: env::var("ORACLE_CONSUMER_GROUP")
                .unwrap_or_else(|_| "shinobi-oracle-reviewer".to_string()),
        }
    }
}
