//! Use Case: ListGitHubRepos — Liste les dépôts GitHub de l'utilisateur (Phase 20B).
//!
//! Utilise le token OAuth stocké pour interroger l'API GitHub
//! et retourne la liste enrichie avec un flag `already_imported`.

use std::sync::Arc;

use serde::Serialize;
use tracing::{info, instrument};
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::github_service::{GitHubRepoInfo, GitHubService};
use domain::ports::repo_repository::RepoRepository;

/// Résultat enrichi d'un repo GitHub.
#[derive(Debug, Clone, Serialize)]
pub struct GitHubRepoWithStatus {
    #[serde(flatten)]
    pub info: GitHubRepoInfo,
    /// `true` si ce repo est déjà importé dans SHINOBI (match sur `mirror_source_url`).
    pub already_imported: bool,
}

/// Use case : lister les repos GitHub de l'utilisateur connecté.
pub struct ListGitHubReposUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    github_service: Arc<dyn GitHubService>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl ListGitHubReposUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        github_service: Arc<dyn GitHubService>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self {
            actor_repo,
            github_service,
            repo_repo,
        }
    }

    #[instrument(skip(self), fields(actor_id = %actor_id))]
    pub async fn execute(
        &self,
        actor_id: &Uuid,
    ) -> Result<Vec<GitHubRepoWithStatus>, DomainError> {
        // 1. Récupérer le github_token de l'acteur
        let token = self
            .actor_repo
            .get_github_token(actor_id)
            .await?
            .ok_or_else(|| {
                DomainError::BusinessRule(
                    "Aucun compte GitHub lié. Connectez-vous via GitHub OAuth d'abord.".to_string(),
                )
            })?;

        // 2. Lister les repos GitHub (max 100, public only)
        let gh_repos = self.github_service.list_user_repos(&token, 100).await?;

        // 3. Récupérer les repos SHINOBI de cet acteur pour le cross-check
        let shinobi_repos = self.repo_repo.list_by_owner(actor_id).await?;
        let imported_urls: Vec<String> = shinobi_repos
            .iter()
            .filter_map(|r| r.mirror_source_url.clone())
            .map(|url| url.to_lowercase())
            .collect();

        // 4. Enrichir avec le flag already_imported
        let results: Vec<GitHubRepoWithStatus> = gh_repos
            .into_iter()
            .map(|info| {
                let already = imported_urls.contains(&info.clone_url.to_lowercase());
                GitHubRepoWithStatus {
                    info,
                    already_imported: already,
                }
            })
            .collect();

        info!(
            count = results.len(),
            already_imported = results.iter().filter(|r| r.already_imported).count(),
            "Phase 20B: Repos GitHub listés avec enrichissement"
        );

        Ok(results)
    }
}
