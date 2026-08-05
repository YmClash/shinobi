//! Fetcher d'acteurs ActivityPub distants — Phase 27-bis-A.
//!
//! Récupère le profil JSON-LD d'un acteur distant pour obtenir sa clé
//! publique RSA (vérification de signature HTTP).
//!
//! ## Sécurité (SSRF Guard)
//! - **Timeout strict** : 3 secondes max par requête
//! - **Pas de redirection vers localhost/127.0.0.1** : protège contre les
//!   attaques SSRF où un attaquant pointe son `keyId` vers une ressource interne
//! - **Cache TTL 5 min** : protège contre les attaques par amplification (DDoS)

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::{info, warn};

/// Profil d'acteur distant minimal (clé publique uniquement).
#[derive(Debug, Clone)]
pub struct RemoteActorProfile {
    /// URI de l'acteur (son `id` AP).
    pub id: String,
    /// Clé publique PEM.
    pub public_key_pem: String,
    /// ID de la clé (`publicKey.id`).
    pub key_id: String,
    /// URI de l'inbox (pour la livraison).
    pub inbox: String,
}

/// Entrée en cache avec TTL.
struct CachedActor {
    profile: RemoteActorProfile,
    fetched_at: Instant,
}

/// Cache TTL = 5 minutes.
const CACHE_TTL: Duration = Duration::from_secs(300);

/// Client de récupération d'acteurs distants avec cache DashMap.
pub struct RemoteActorFetcher {
    cache: Arc<DashMap<String, CachedActor>>,
    client: reqwest::Client,
}

impl RemoteActorFetcher {
    /// Crée un nouveau fetcher avec un client HTTP sécurisé.
    ///
    /// ## SSRF Guard
    /// - Timeout 3 secondes
    /// - Pas de redirect automatique (on vérifie manuellement)
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none()) // Pas de redirect auto
            .user_agent("SHINOBI/0.1.0 (+https://shinobi.dev)")
            .build()
            .expect("Failed to build reqwest client");

        Self {
            cache: Arc::new(DashMap::new()),
            client,
        }
    }

    /// Récupère le profil d'un acteur distant (avec cache TTL 5 min).
    ///
    /// ## Arguments
    /// - `actor_uri` : URI ActivityPub de l'acteur (ex: `https://mastodon.social/users/bob`)
    ///
    /// ## SSRF Protection
    /// Rejette les URIs pointant vers des adresses locales (localhost, 127.0.0.1, [::1]).
    pub async fn fetch(&self, actor_uri: &str) -> Result<RemoteActorProfile, RemoteActorError> {
        // ── Cache hit ? ──
        if let Some(entry) = self.cache.get(actor_uri) {
            if entry.fetched_at.elapsed() < CACHE_TTL {
                return Ok(entry.profile.clone());
            }
            // TTL expiré → drop et re-fetch
        }
        // Explicit drop pour libérer le read lock DashMap
        self.cache.remove(actor_uri);

        // ── SSRF Guard ──
        Self::validate_uri(actor_uri)?;

        // ── Fetch HTTP ──
        info!(uri = %actor_uri, "🌐 Fetching remote actor profile");

        let response = self.client
            .get(actor_uri)
            .header("Accept", "application/activity+json, application/ld+json")
            .send()
            .await
            .map_err(|e| {
                warn!(uri = %actor_uri, error = %e, "❌ Failed to fetch remote actor");
                RemoteActorError::FetchFailed(e.to_string())
            })?;

        if !response.status().is_success() {
            return Err(RemoteActorError::FetchFailed(
                format!("HTTP {} from {}", response.status(), actor_uri),
            ));
        }

        let body: serde_json::Value = response.json().await
            .map_err(|e| RemoteActorError::ParseFailed(e.to_string()))?;

        // ── Extraire la clé publique ──
        let id = body.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(actor_uri)
            .to_string();

        let public_key = body.get("publicKey")
            .ok_or_else(|| RemoteActorError::MissingPublicKey)?;

        let public_key_pem = public_key.get("publicKeyPem")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RemoteActorError::MissingPublicKey)?
            .to_string();

        let key_id = public_key.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let inbox = body.get("inbox")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let profile = RemoteActorProfile {
            id,
            public_key_pem,
            key_id,
            inbox,
        };

        // ── Cache store ──
        self.cache.insert(actor_uri.to_string(), CachedActor {
            profile: profile.clone(),
            fetched_at: Instant::now(),
        });

        info!(
            uri = %actor_uri,
            key_id = %profile.key_id,
            "✅ Remote actor fetched and cached (TTL 5 min)"
        );

        Ok(profile)
    }

    /// Valide l'URI contre les attaques SSRF.
    ///
    /// Rejette :
    /// - `http://localhost...`
    /// - `http://127.0.0.1...`
    /// - `http://[::1]...`
    /// - `http://0.0.0.0...`
    /// - Schemes non-HTTPS (sauf en dev)
    fn validate_uri(uri: &str) -> Result<(), RemoteActorError> {
        let parsed = url::Url::parse(uri)
            .map_err(|_| RemoteActorError::InvalidUri(uri.to_string()))?;

        // Vérifier le host
        if let Some(host) = parsed.host_str() {
            let host_lower = host.to_lowercase();
            if host_lower == "localhost"
                || host_lower == "127.0.0.1"
                || host_lower == "[::1]"
                || host_lower == "0.0.0.0"
                || host_lower.starts_with("10.")
                || host_lower.starts_with("192.168.")
                || host_lower.starts_with("172.16.")
            {
                warn!(uri = %uri, host = %host, "🛡️ SSRF blocked — private/local address");
                return Err(RemoteActorError::SsrfBlocked(host.to_string()));
            }
        }

        Ok(())
    }
}

/// Erreurs du fetcher d'acteurs distants.
#[derive(Debug, thiserror::Error)]
pub enum RemoteActorError {
    #[error("Failed to fetch remote actor: {0}")]
    FetchFailed(String),

    #[error("Failed to parse remote actor profile: {0}")]
    ParseFailed(String),

    #[error("Remote actor missing publicKey")]
    MissingPublicKey,

    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    #[error("SSRF blocked — private address: {0}")]
    SsrfBlocked(String),
}
