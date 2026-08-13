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

    /// URL du serveur Ollama Docker (LLM local — Oracle). Défaut: "http://localhost:11435".
    pub ollama_url: String,

    /// Modèle Ollama à utiliser (Oracle). Défaut: "granite3.1-dense:2b".
    pub ollama_model: String,

    /// Activer/désactiver le consumer Oracle. Défaut: true.
    pub oracle_consumer_enabled: bool,

    /// Consumer group Kafka pour l'agent Oracle. Défaut: "shinobi-oracle-reviewer".
    pub oracle_consumer_group: String,

    // ── Phase 15 — Sensei (先生) ──────────────────────────────────────
    /// Activer/désactiver l'agent Sensei (chat conversationnel). Défaut: true.
    pub sensei_enabled: bool,

    /// URL du serveur Ollama #2 pour Sensei (instance séparée de l'Oracle).
    /// Défaut: "http://localhost:11436".
    pub sensei_ollama_url: String,

    /// Modèle Ollama pour Sensei (conversationnel, spécialisé code).
    /// Défaut: "qwen2.5-coder:7b".
    pub sensei_ollama_model: String,

    // ── Phase 19A — Auth & RBAC ───────────────────────────────────────
    /// Secret JWT pour signer les tokens (HS256). ≥32 caractères recommandé.
    /// Défaut: "shinobi-dev-secret-change-me-in-production!" (dev uniquement).
    pub jwt_secret: String,

    /// Durée de validité des JWT en secondes. Défaut: 604800 (7 jours).
    pub jwt_duration_secs: i64,

    // ── Phase 20 — GitHub OAuth ────────────────────────────────────
    /// GitHub OAuth Application Client ID (optionnel).
    /// Si absent, les routes OAuth sont désactivées (graceful degradation).
    pub github_client_id: Option<String>,

    /// GitHub OAuth Application Client Secret (optionnel).
    pub github_client_secret: Option<String>,

    // ── Phase 27 — ForgeFed (Fédération) ──────────────────────
    /// Domaine public de l'instance pour la fédération ActivityPub.
    /// Utilisé pour construire les URIs ActivityPub (ex: `https://{domain}/actors/{handle}`).
    /// Défaut: "localhost:3000" (dev).
    pub federation_domain: String,

    /// Activer/désactiver la fédération ActivityPub. Défaut: false.
    pub federation_enabled: bool,
}

impl Config {
    /// Charge la configuration depuis les variables d'environnement.
    ///
    /// Collecte toutes les erreurs de validation et les rapporte
    /// dans un seul message clair au lieu de paniquer sur la première
    /// variable manquante.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut errors: Vec<String> = Vec::new();

        // ── Variables critiques (sans défaut) ──────────────────
        let database_url = require_env("DATABASE_URL", &mut errors);

        // ── Variables avec défaut ──────────────────────────────
        let config = Self {
            database_url: database_url.unwrap_or_default(),
            redis_url: env_or("REDIS_URL", "redis://127.0.0.1:6379"),
            rest_port: env_parse("REST_PORT", 3000),
            grpc_port: env_parse("GRPC_PORT", 50051),
            vcs_workspace_root: env_or("VCS_WORKSPACE_ROOT", "./workspace"),
            kafka_brokers: env_or("KAFKA_BROKERS", "localhost:9092"),
            kafka_topic: env_or("KAFKA_TOPIC", "shinobi.vcs.operations"),
            kafka_analysis_topic: env_or("KAFKA_ANALYSIS_TOPIC", "shinobi.tensai.analysis-complete"),
            ipfs_api_url: env_or("IPFS_API_URL", "http://127.0.0.1:5001"),
            kafka_consumer_group: env_or("KAFKA_CONSUMER_GROUP", "shinobi-tensai-analyzer"),
            tensai_consumer_enabled: env_bool("TENSAI_CONSUMER_ENABLED", true),
            embedding_enabled: env_bool("EMBEDDING_ENABLED", true),
            embedding_dimensions: env_parse("EMBEDDING_DIMENSIONS", 256),
            ollama_url: env_or("OLLAMA_URL", "http://localhost:11435"),
            ollama_model: env_or("OLLAMA_MODEL", "granite3-dense:2b"),
            oracle_consumer_enabled: env_bool("ORACLE_CONSUMER_ENABLED", true),
            oracle_consumer_group: env_or("ORACLE_CONSUMER_GROUP", "shinobi-oracle-reviewer"),
            // ── Phase 15 — Sensei ────────────────────────
            sensei_enabled: env_bool("SENSEI_ENABLED", true),
            sensei_ollama_url: env_or("SENSEI_OLLAMA_URL", "http://localhost:11436"),
            sensei_ollama_model: env_or("SENSEI_OLLAMA_MODEL", "smollm2:1.7b"),
            // ── Phase 19A — Auth ────────────────────────
            jwt_secret: env_or("JWT_SECRET", "shinobi-dev-secret-change-me-in-production!"),
            jwt_duration_secs: env_parse("JWT_DURATION_SECS", 604_800), // 7 days
            // ── Phase 20 — GitHub OAuth ────────────────────
            github_client_id: env::var("GITHUB_CLIENT_ID").ok(),
            github_client_secret: env::var("GITHUB_CLIENT_SECRET").ok(),
            // ── Phase 27 — ForgeFed ────────────────────────
            federation_domain: sanitize_federation_domain(
                &env_or("FEDERATION_DOMAIN", "localhost:3000"),
            ),
            federation_enabled: env_bool("FEDERATION_ENABLED", false),
        };

        if !errors.is_empty() {
            return Err(ConfigError {
                missing_vars: errors,
            });
        }

        Ok(config)
    }

    /// Affiche un résumé de la configuration au démarrage.
    /// Les secrets sont masqués.
    pub fn log_summary(&self) {
        eprintln!("┌───────────────────────────────────────────────────────────────────────────────");
        eprintln!("│ ⚙️  Configuration Taijutsu");
        eprintln!("├───────────────────────────────────────────────────────────────────────────────");
        eprintln!("│ REST port       : {}", self.rest_port);
        eprintln!("│ gRPC port       : {}", self.grpc_port);
        eprintln!("│ Database        : {}...{}", &self.database_url[..self.database_url.find('@').unwrap_or(20).min(20)], &self.database_url[self.database_url.rfind('/').unwrap_or(0)..]);
        eprintln!("│ Redis           : {}", self.redis_url);
        eprintln!("│ VCS root        : {}", self.vcs_workspace_root);
        eprintln!("│ Kafka           : {}", self.kafka_brokers);
        eprintln!("│ IPFS            : {}", self.ipfs_api_url);
        eprintln!("│ Ollama (Oracle) : {} ({})", self.ollama_url, self.ollama_model);
        eprintln!("│ Ollama (Sensei) : {} ({})", self.sensei_ollama_url, self.sensei_ollama_model);
        eprintln!("│ JWT secret      : {}...", &self.jwt_secret[..8.min(self.jwt_secret.len())]);
        eprintln!("│ Federation      : {} ({})", self.federation_domain, if self.federation_enabled { "enabled" } else { "disabled" });
        eprintln!("│ GitHub OAuth    : {}", if self.github_client_id.is_some() { "configured" } else { "not configured" });
        eprintln!("│ Tensai          : {}", if self.tensai_consumer_enabled { "enabled" } else { "disabled" });
        eprintln!("│ Oracle          : {}", if self.oracle_consumer_enabled { "enabled" } else { "disabled" });
        eprintln!("│ Sensei          : {}", if self.sensei_enabled { "enabled" } else { "disabled" });
        eprintln!("│ Embedding       : {} ({}d)", if self.embedding_enabled { "enabled" } else { "disabled" }, self.embedding_dimensions);
        eprintln!("└───────────────────────────────────────────────────────────────────────────────");
    }
}

// ── Config Error ──────────────────────────────────────────────────────

/// Erreur de configuration — variables d'environnement manquantes.
#[derive(Debug)]
pub struct ConfigError {
    pub missing_vars: Vec<String>,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "❌ Configuration invalide — variables manquantes :")?;
        for var in &self.missing_vars {
            writeln!(f, "   • {}", var)?;
        }
        writeln!(f, "\n   Consultez .env.example pour la liste complète.")
    }
}

impl std::error::Error for ConfigError {}

// ── Helper functions ──────────────────────────────────────────────────

/// Lit une variable d'environnement requise. Ajoute une erreur si absente.
fn require_env(key: &str, errors: &mut Vec<String>) -> Option<String> {
    match env::var(key) {
        Ok(val) if !val.trim().is_empty() => Some(val),
        _ => {
            errors.push(format!("{} (requis, pas de défaut)", key));
            None
        }
    }
}

/// Lit une variable d'environnement avec valeur par défaut.
fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Lit et parse une variable d'environnement numérique avec valeur par défaut.
fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Lit une variable d'environnement booléenne (false/0 = false, sinon = défaut).
fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .map(|v| v != "false" && v != "0")
        .unwrap_or(default)
}

/// Sanitise le domaine fédéré en supprimant schéma, espaces et trailing slash.
///
/// Le `FEDERATION_DOMAIN` doit être un domaine nu (ex: `forge.shinobi.dev`)
/// sans schéma `https://`. Cette fonction corrige les erreurs de copier-coller
/// fréquentes dans `.env` (espaces, schéma inclus, trailing slash).
///
/// ## Exemples
/// - `" https://forge.shinobi.dev "` → `"forge.shinobi.dev"`
/// - `"http://localhost:3000/"` → `"localhost:3000"`
/// - `"localhost:3000"` → `"localhost:3000"` (inchangé)
fn sanitize_federation_domain(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let clean = without_scheme.trim_end_matches('/');
    if clean != trimmed {
        eprintln!(
            "⚠️  FEDERATION_DOMAIN sanitized: {:?} → {:?} (stripped scheme/spaces/slash)",
            raw, clean
        );
    }
    clean.to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_plain_domain() {
        assert_eq!(sanitize_federation_domain("localhost:3000"), "localhost:3000");
    }

    #[test]
    fn test_sanitize_strips_https_scheme() {
        assert_eq!(
            sanitize_federation_domain("https://forge.shinobi.dev"),
            "forge.shinobi.dev"
        );
    }

    #[test]
    fn test_sanitize_strips_http_scheme() {
        assert_eq!(
            sanitize_federation_domain("http://localhost:3000"),
            "localhost:3000"
        );
    }

    #[test]
    fn test_sanitize_strips_spaces_and_scheme() {
        assert_eq!(
            sanitize_federation_domain(" https://reliably-recognize-payback.ngrok-free.dev "),
            "reliably-recognize-payback.ngrok-free.dev"
        );
    }

    #[test]
    fn test_sanitize_strips_trailing_slash() {
        assert_eq!(
            sanitize_federation_domain("https://forge.shinobi.dev/"),
            "forge.shinobi.dev"
        );
    }
}

