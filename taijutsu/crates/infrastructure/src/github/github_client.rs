//! Adaptateur GitHub — Client HTTP pour l'API GitHub v3 + Git Fetch Injecté.
//!
//! ## Phase 19B — Le Pont des Mondes
//! Ce module implémente le port `GitHubService` :
//! - `fetch_repo_info()` → API REST GitHub `/repos/{owner}/{repo}`
//! - `fetch_into_bare()` → `git remote add` + `git fetch +refs/*:refs/*`
//!
//! ## Le Fetch Injecté (vs rm -rf + clone --mirror)
//! Au lieu de détruire le bare git repo créé par jj-lib puis le cloner,
//! on injecte les objets Git dans le store existant :
//! 1. `git remote add origin {clone_url}` dans le bare repo jj
//! 2. `git fetch origin +refs/*:refs/*` — aspire toutes les branches/tags
//!
//! Cette méthode préserve la structure interne de Jujutsu (HEAD, config).

use std::path::Path;

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{info, instrument, warn};

use domain::errors::DomainError;
use domain::ports::github_service::{GitHubRepoInfo, GitHubService};

/// Client GitHub utilisant `reqwest` pour l'API REST
/// et `tokio::process::Command` pour les opérations Git.
pub struct GitHubClient {
    http: reqwest::Client,
}

impl GitHubClient {
    /// Construit un nouveau client GitHub avec un User-Agent conforme.
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("SHINOBI-VCS/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build reqwest client");

        Self { http }
    }
}

/// Réponse JSON partielle de l'API GitHub `/repos/{owner}/{repo}`.
#[derive(Debug, Deserialize)]
struct GitHubApiRepo {
    full_name: String,
    name: String,
    description: Option<String>,
    clone_url: String,
    default_branch: String,
    stargazers_count: u32,
    forks_count: u32,
    language: Option<String>,
    private: bool,
    license: Option<GitHubApiLicense>,
}

#[derive(Debug, Deserialize)]
struct GitHubApiLicense {
    spdx_id: Option<String>,
}

#[async_trait]
impl GitHubService for GitHubClient {
    #[instrument(skip(self), fields(github = %format!("{owner}/{repo}")))]
    async fn fetch_repo_info(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<GitHubRepoInfo, DomainError> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}");

        let response = self
            .http
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| {
                DomainError::External(format!("GitHub API request failed: {e}"))
            })?;

        let status = response.status();

        // Rate limit (403) — passthrough pragmatique
        if status.as_u16() == 403 {
            let body = response.text().await.unwrap_or_default();
            if body.contains("rate limit") || body.contains("API rate limit") {
                return Err(DomainError::External(
                    "GitHub API rate limit reached. Try again later.".to_string(),
                ));
            }
            return Err(DomainError::External(format!(
                "GitHub API returned 403: {body}"
            )));
        }

        // Not found (404) — repo privé ou inexistant
        if status.as_u16() == 404 {
            return Err(DomainError::NotFound {
                entity_type: "GitHub Repository",
                id: uuid::Uuid::nil(),
            });
        }

        // Autres erreurs HTTP
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DomainError::External(format!(
                "GitHub API returned {status}: {body}"
            )));
        }

        let api_repo: GitHubApiRepo = response.json().await.map_err(|e| {
            DomainError::External(format!("GitHub API JSON parse failed: {e}"))
        })?;

        let info = GitHubRepoInfo {
            full_name: api_repo.full_name,
            name: api_repo.name,
            description: api_repo.description,
            clone_url: api_repo.clone_url,
            default_branch: api_repo.default_branch,
            stars: api_repo.stargazers_count,
            forks: api_repo.forks_count,
            language: api_repo.language,
            license: api_repo.license.and_then(|l| l.spdx_id),
            is_private: api_repo.private,
        };

        info!(
            full_name = %info.full_name,
            stars = info.stars,
            language = ?info.language,
            "GitHub repo info fetched successfully"
        );

        Ok(info)
    }

    #[instrument(skip(self), fields(clone_url = %clone_url, target = %bare_repo_path.display()))]
    async fn fetch_into_bare(
        &self,
        clone_url: &str,
        bare_repo_path: &Path,
    ) -> Result<(), DomainError> {
        // 1. Ajouter le remote origin dans le bare repo existant
        let add_remote = tokio::process::Command::new("git")
            .current_dir(bare_repo_path)
            .args(["remote", "add", "origin", clone_url])
            .output()
            .await
            .map_err(|e| {
                DomainError::VcsError(format!("Failed to spawn git remote add: {e}"))
            })?;

        if !add_remote.status.success() {
            let stderr = String::from_utf8_lossy(&add_remote.stderr);
            // Ignorer "remote origin already exists" (idempotent)
            if !stderr.contains("already exists") {
                return Err(DomainError::VcsError(format!(
                    "git remote add failed: {stderr}"
                )));
            }
            warn!(
                "git remote origin already exists — skipping add (idempotent)"
            );
        }

        info!(
            clone_url = %clone_url,
            "Remote origin added to bare repo"
        );

        // 2. Fetch toutes les refs (+refs/*:refs/*) — Le Fetch Injecté
        let fetch_output = tokio::process::Command::new("git")
            .current_dir(bare_repo_path)
            .args(["fetch", "origin", "+refs/*:refs/*", "--prune"])
            .output()
            .await
            .map_err(|e| {
                DomainError::VcsError(format!("Failed to spawn git fetch: {e}"))
            })?;

        if !fetch_output.status.success() {
            let stderr = String::from_utf8_lossy(&fetch_output.stderr);
            return Err(DomainError::VcsError(format!(
                "git fetch from GitHub failed: {stderr}"
            )));
        }

        let stderr_info = String::from_utf8_lossy(&fetch_output.stderr);
        info!(
            clone_url = %clone_url,
            fetch_output = %stderr_info.chars().take(200).collect::<String>(),
            "GitHub repo content fetched into bare repo (Le Fetch Injecté)"
        );

        Ok(())
    }
}
