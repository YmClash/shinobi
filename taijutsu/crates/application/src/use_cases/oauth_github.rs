//! Use Case: OAuthGitHub — Authentification via GitHub OAuth (Phase 20).
//!
//! Pipeline sécurisé en 11 étapes :
//! 1. Vérifier le state anti-CSRF (Redis)
//! 2. Supprimer le state (one-time use)
//! 3. Échanger le code → access_token (POST github.com)
//! 4. GET /user → profil GitHub (id, login, name, avatar_url)
//! 5. GET /user/emails → trouver email primary+verified
//! 6. SECURITY: Rejeter si aucun email verified pour le lien auto
//! 7. Chercher Actor par github_id → login direct
//! 8. Chercher Actor par email vérifié → lier github_id + login
//! 9. Sinon → créer nouvel Actor
//! 10. Sauvegarder credential oauth_token
//! 11. Générer JWT SHINOBI

use std::sync::Arc;

use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn, instrument};

use domain::entities::actor::{Actor, ActorType};
use domain::entities::session::AuthClaims;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::auth_service::AuthService;

use infrastructure::cache::redis_cache::RedisCache;

// ── Types GitHub API ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUserProfile {
    id: i64,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
    #[allow(dead_code)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

// ── Commande & Résultat ───────────────────────────────────────────────

#[derive(Debug)]
pub struct OAuthGitHubCommand {
    pub code: String,
    pub state: String,
}

#[derive(Debug)]
pub struct OAuthGitHubResult {
    pub actor: Actor,
    pub token: String,
    pub is_new_account: bool,
}

// ── Use Case ──────────────────────────────────────────────────────────

pub struct OAuthGitHubUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    auth_service: Arc<dyn AuthService>,
    redis: RedisCache,
    http_client: Client,
    client_id: String,
    client_secret: String,
    jwt_duration_secs: i64,
}

impl OAuthGitHubUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        auth_service: Arc<dyn AuthService>,
        redis: RedisCache,
        client_id: String,
        client_secret: String,
        jwt_duration_secs: i64,
    ) -> Self {
        Self {
            actor_repo,
            auth_service,
            redis,
            http_client: Client::new(),
            client_id,
            client_secret,
            jwt_duration_secs,
        }
    }

    /// Génère un state anti-CSRF, le stocke en Redis (TTL 10min), retourne l'URL GitHub.
    pub async fn generate_auth_url(&self, redirect_uri: &str) -> Result<(String, String), DomainError> {
        use rand::Rng;
        use rand::distr::Alphanumeric;

        // Générer un state aléatoire (32 chars alphanumeric)
        let state: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        // Stocker en Redis (TTL 600 secondes = 10 minutes)
        let redis_key = format!("oauth_state:{state}");
        self.redis
            .set(&redis_key, "pending", Some(600))
            .await
            .map_err(|e| DomainError::Persistence(format!("Redis SET error: {e}")))?;

        let url = format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=read:user,user:email&state={}",
            self.client_id,
            urlencoding::encode(redirect_uri),
            state,
        );

        Ok((url, state))
    }

    #[instrument(skip(self, cmd), fields(state = %cmd.state))]
    pub async fn execute(&self, cmd: OAuthGitHubCommand) -> Result<OAuthGitHubResult, DomainError> {
        // ── 1. Vérifier le state anti-CSRF ──────────────────────
        let redis_key = format!("oauth_state:{}", cmd.state);
        let state_exists = self
            .redis
            .get(&redis_key)
            .await
            .map_err(|e| DomainError::Persistence(format!("Redis GET error: {e}")))?;

        if state_exists.is_none() {
            warn!(state = %cmd.state, "🛡️ CSRF détecté — state invalide ou expiré");
            return Err(DomainError::Unauthorized(
                "Session OAuth invalide ou expirée (CSRF protection). Réessayez.".to_string(),
            ));
        }

        // ── 2. Supprimer le state (one-time use) ────────────────
        // Overwrite with 1-second TTL to effectively delete
        let _ = self.redis.set(&redis_key, "used", Some(1)).await;

        // ── 3. Échanger le code → access_token ──────────────────
        let token_resp = self
            .http_client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")  // ⚡ CRITIQUE: sans cela GitHub répond en URL-encoded
            .json(&serde_json::json!({
                "client_id": self.client_id,
                "client_secret": self.client_secret,
                "code": cmd.code,
            }))
            .send()
            .await
            .map_err(|e| DomainError::External(format!("GitHub token exchange failed: {e}")))?;

        if !token_resp.status().is_success() {
            let body = token_resp.text().await.unwrap_or_default();
            return Err(DomainError::External(format!(
                "GitHub token exchange error: {body}"
            )));
        }

        let token_data: GitHubTokenResponse = token_resp
            .json()
            .await
            .map_err(|e| DomainError::External(format!("GitHub token parse error: {e}")))?;

        let access_token = &token_data.access_token;

        // ── 4. GET /user → profil GitHub ────────────────────────
        let profile: GitHubUserProfile = self
            .http_client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "SHINOBI-Forge/1.0")
            .send()
            .await
            .map_err(|e| DomainError::External(format!("GitHub /user failed: {e}")))?
            .json()
            .await
            .map_err(|e| DomainError::External(format!("GitHub /user parse: {e}")))?;

        info!(github_id = profile.id, login = %profile.login, "🔑 Profil GitHub récupéré");

        // ── 5. GET /user/emails → email primary+verified ────────
        let emails: Vec<GitHubEmail> = self
            .http_client
            .get("https://api.github.com/user/emails")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "SHINOBI-Forge/1.0")
            .send()
            .await
            .map_err(|e| DomainError::External(format!("GitHub /user/emails failed: {e}")))?
            .json()
            .await
            .map_err(|e| DomainError::External(format!("GitHub /user/emails parse: {e}")))?;

        // ── 6. SECURITY: Seul email verified+primary accepté ────
        let verified_email = emails
            .iter()
            .find(|e| e.primary && e.verified)
            .map(|e| e.email.clone());

        if verified_email.is_none() {
            warn!(github_id = profile.id, "⚠️ Aucun email vérifié sur le compte GitHub");
        }

        // ── 7. Chercher Actor par github_id ─────────────────────
        if let Some(existing) = self.actor_repo.find_by_github_id(profile.id).await? {
            info!(actor_id = %existing.id, "✅ Login GitHub direct (github_id match)");
            let token = self.generate_jwt_for(&existing)?;
            return Ok(OAuthGitHubResult {
                actor: existing,
                token,
                is_new_account: false,
            });
        }

        // ── 8. Chercher Actor par email vérifié → lier ──────────
        if let Some(ref email) = verified_email {
            if let Some(existing) = self.actor_repo.find_by_email(email).await? {
                // Lier le github_id au compte existant
                self.actor_repo
                    .update_github_id(&existing.id, profile.id)
                    .await?;

                info!(
                    actor_id = %existing.id,
                    email = %email,
                    github_id = profile.id,
                    "🔗 Compte GitHub lié à l'acteur existant (email vérifié match)"
                );

                let token = self.generate_jwt_for(&existing)?;
                return Ok(OAuthGitHubResult {
                    actor: existing,
                    token,
                    is_new_account: false,
                });
            }
        }

        // ── 9. Créer un nouvel Actor ────────────────────────────
        let handle = self.generate_unique_handle(&profile.login).await?;
        let display_name = profile.name.unwrap_or_else(|| profile.login.clone());

        let mut actor = Actor::new(&handle, &display_name, ActorType::Human);
        actor.email = verified_email;
        actor.avatar_url = profile.avatar_url;
        actor.github_id = Some(profile.id);

        self.actor_repo.save(&actor).await?;

        info!(
            actor_id = %actor.id,
            handle = %actor.handle,
            github_id = profile.id,
            "🌟 Nouvel acteur créé via GitHub OAuth"
        );

        // ── 10. Sauvegarder credential oauth_token ──────────────
        let token_hash = self.auth_service.hash_pat_for_lookup(access_token);
        self.actor_repo
            .save_credential(&actor.id, "oauth_token", &token_hash, None, Some("github"))
            .await?;

        // ── 11. Générer JWT SHINOBI ─────────────────────────────
        let token = self.generate_jwt_for(&actor)?;

        Ok(OAuthGitHubResult {
            actor,
            token,
            is_new_account: true,
        })
    }

    // ── Helpers ───────────────────────────────────────────────────

    fn generate_jwt_for(&self, actor: &Actor) -> Result<String, DomainError> {
        let claims = AuthClaims::new(
            actor.id,
            actor.handle.clone(),
            actor.actor_type.clone(),
            self.jwt_duration_secs,
        );
        self.auth_service.generate_jwt(&claims)
    }

    /// Génère un handle unique à partir du login GitHub.
    async fn generate_unique_handle(&self, github_login: &str) -> Result<String, DomainError> {
        let base: String = github_login
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
            .collect();

        let base = if base.is_empty() { "github-user".to_string() } else { base };

        if self.actor_repo.find_by_handle(&base).await?.is_none() {
            return Ok(base);
        }

        for i in 1..100 {
            let candidate = format!("{base}-{i}");
            if self.actor_repo.find_by_handle(&candidate).await?.is_none() {
                return Ok(candidate);
            }
        }

        Err(DomainError::BusinessRule(
            "Impossible de générer un handle unique".to_string(),
        ))
    }
}
