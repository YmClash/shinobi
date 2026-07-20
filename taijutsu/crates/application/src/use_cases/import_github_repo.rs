//! Use Case: ImportGitHubRepo — Import d'un dépôt GitHub dans la Forge SHINOBI.
//!
//! ## Phase 19B — Le Pont des Mondes
//!
//! Orchestre le pipeline complet d'import :
//! 1. Parse l'URL GitHub → (owner, repo)
//! 2. Fetch metadata via l'API GitHub v3
//! 3. Vérifie que le repo est public (V1)
//! 4. Crée le Repository SHINOBI avec les metadata mirror
//! 5. Init le workspace VCS (Jujutsu + bare Git)
//! 6. Inject le contenu GitHub via `fetch_into_bare` (Le Fetch Injecté)
//! 7. Reload jj + import refs Git
//! 8. Sync Hook HEAD-only (PostgreSQL + IPFS + Kafka)
//! 9. Update mirror_synced_at
//!
//! ## Sync Hook HEAD-only
//! Au lieu de créer une Operation pour chaque commit de l'historique GitHub
//! (ce qui causerait un DDoS sur Kafka/Ollama pour les gros repos),
//! on ne publie l'événement que pour le commit HEAD. Tensai indexe
//! le code actuel en quelques secondes.

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::entities::repository::Repository;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::content_store::ContentStore;
use domain::ports::event_publisher::EventPublisher;
use domain::ports::github_service::GitHubService;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::VcsEngine;

use infrastructure::vcs::jujutsu_engine::JujutsuEngine;

/// Commande d'import d'un dépôt GitHub.
#[derive(Debug, Clone)]
pub struct ImportGitHubRepoCommand {
    /// ID de l'acteur SHINOBI qui importe le repo.
    pub owner_id: Uuid,
    /// URL du dépôt GitHub (ex: "https://github.com/tokio-rs/tokio").
    pub github_url: String,
    /// Override du nom de repo dans SHINOBI (défaut: nom GitHub).
    pub name_override: Option<String>,
}

/// Use case d'import de dépôt GitHub dans la Forge SHINOBI.
pub struct ImportGitHubRepoUseCase {
    github_service: Arc<dyn GitHubService>,
    actor_repo: Arc<dyn ActorRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    vcs_engine: Arc<JujutsuEngine>,
    operation_repo: Arc<dyn OperationRepository>,
    content_store: Option<Arc<dyn ContentStore>>,
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl ImportGitHubRepoUseCase {
    pub fn new(
        github_service: Arc<dyn GitHubService>,
        actor_repo: Arc<dyn ActorRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        vcs_engine: Arc<JujutsuEngine>,
        operation_repo: Arc<dyn OperationRepository>,
        content_store: Option<Arc<dyn ContentStore>>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self {
            github_service,
            actor_repo,
            repo_repo,
            vcs_engine,
            operation_repo,
            content_store,
            event_publisher,
        }
    }

    #[instrument(skip(self), fields(owner_id = %cmd.owner_id, github_url = %cmd.github_url))]
    pub async fn execute(
        &self,
        cmd: ImportGitHubRepoCommand,
    ) -> Result<Repository, DomainError> {
        // 1. Parser l'URL GitHub
        let (gh_owner, gh_repo) = parse_github_url(&cmd.github_url)?;

        info!(
            github_owner = %gh_owner,
            github_repo = %gh_repo,
            "Phase 19B: Import GitHub démarré"
        );

        // 2. Fetch metadata via l'API GitHub v3
        let gh_info = self
            .github_service
            .fetch_repo_info(&gh_owner, &gh_repo)
            .await?;

        // 3. Rejeter les repos privés (V1 — Public only)
        if gh_info.is_private {
            return Err(DomainError::BusinessRule(
                "L'import de dépôts privés GitHub n'est pas supporté en V1. \
                 Seuls les dépôts publics peuvent être importés."
                    .to_string(),
            ));
        }

        // 4. Vérifier que l'acteur SHINOBI existe
        let _actor = self
            .actor_repo
            .find_by_id(&cmd.owner_id)
            .await?
            .ok_or(DomainError::NotFound {
                entity_type: "Actor",
                id: cmd.owner_id,
            })?;

        // 5. Créer le Repository SHINOBI avec metadata mirror
        let repo_name = cmd.name_override.unwrap_or_else(|| gh_repo.clone());
        let mut repo = Repository::new(
            cmd.owner_id,
            &repo_name,
            &gh_info.full_name,
        );
        repo.description = gh_info.description.clone();
        repo.default_branch = gh_info.default_branch.clone();
        repo.mirror_source_url = Some(gh_info.clone_url.clone());

        self.repo_repo.save(&repo).await?;
        self.repo_repo
            .add_collaborator(&cmd.owner_id, &repo.id, "owner")
            .await?;

        info!(
            repo_id = %repo.id,
            repo_name = %repo_name,
            github_full_name = %gh_info.full_name,
            "Repository SHINOBI créé avec metadata mirror"
        );

        // 6. Init le workspace VCS (Phase 21: owner_id/repo_id)
        self.vcs_engine.init_workspace(&cmd.owner_id, &repo.id).await?;

        // 7. Le Fetch Injecté — aspire le contenu GitHub dans le bare Git repo
        let git_path = self.vcs_engine.git_repo_path(&repo.id);
        self.github_service
            .fetch_into_bare(&gh_info.clone_url, &git_path)
            .await?;

        info!(
            repo_id = %repo.id,
            "Fetch Injecté terminé — contenu GitHub injecté dans le bare repo"
        );

        // 8. Reload jj + import refs Git
        self.vcs_engine.reload_repo(&repo.id).await?;

        // 9. Sync Hook HEAD-only — crée une seule Operation pour le HEAD
        if let Err(e) = self
            .sync_hook_head_only(&repo)
            .await
        {
            // Non-fatal : le repo est créé et fonctionnel même sans Operation
            tracing::warn!(
                repo_id = %repo.id,
                error = %e,
                "Sync Hook HEAD failed (non-fatal) — le repo est fonctionnel"
            );
        }

        // 10. Update mirror_synced_at
        self.repo_repo
            .update_mirror_synced_at(&repo.id)
            .await?;

        info!(
            repo_id = %repo.id,
            github_url = %cmd.github_url,
            "Phase 19B: Import GitHub terminé avec succès 🌉"
        );

        Ok(repo)
    }

    /// Sync Hook simplifié pour le HEAD uniquement.
    ///
    /// Ne crée qu'une seule Operation pour le commit HEAD du repo importé,
    /// évitant un DDoS Kafka/Ollama sur les repos avec des milliers de commits.
    async fn sync_hook_head_only(
        &self,
        repository: &Repository,
    ) -> Result<(), DomainError> {
        let repo_id = repository.id;

        // Résoudre le HEAD Git
        let content_id = self
            .vcs_engine
            .resolve_git_head(&repo_id)
            .ok_or_else(|| {
                DomainError::VcsError(
                    "No git HEAD after import — empty repo?".to_string(),
                )
            })?;

        info!(
            repo_id = %repo_id,
            head = %content_id,
            "Import Sync Hook: HEAD Git résolu"
        );

        // Lire le snapshot du commit HEAD
        let snapshot = self
            .vcs_engine
            .read_commit_snapshot(&repo_id, &content_id)
            .await?;

        let description = if snapshot.description.trim().is_empty() {
            "GitHub import (HEAD)".to_string()
        } else {
            snapshot.description.trim().to_string()
        };

        // Stocker sur IPFS (graceful degradation)
        let ipfs_cid = if let Some(store) = &self.content_store {
            if !snapshot.files.is_empty() {
                match store.store_dag(&description, &snapshot.files).await {
                    Ok((root_cid, _manifest)) => {
                        info!(
                            ipfs_cid = %root_cid,
                            file_count = snapshot.files.len(),
                            "Import Sync Hook: Merkle DAG stocké sur IPFS"
                        );
                        Some(root_cid)
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Import Sync Hook: IPFS échoué (graceful degradation)"
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Construire l'Operation (pas de parents — HEAD importé est orphelin dans SHINOBI)
        let operation = Operation::new(
            repository.owner_id,
            repository.id,
            content_id,
            ipfs_cid,
            &description,
            vec![], // Pas de lignée parentale pour un import HEAD-only
        );

        // Persister dans PostgreSQL
        match self.operation_repo.save(&operation).await {
            Ok(()) => {
                info!(
                    operation_id = %operation.id,
                    "Import Sync Hook: Operation persistée"
                );
            }
            Err(DomainError::Duplicate(_)) => {
                info!("Import Sync Hook: content_id déjà présent — skip");
                return Ok(());
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("duplicate key") || err_str.contains("unique constraint") {
                    info!("Import Sync Hook: content_id déjà présent (contrainte) — skip");
                    return Ok(());
                }
                return Err(e);
            }
        }

        // Publier l'étincelle Kafka (fire-and-forget)
        if let Some(publisher) = &self.event_publisher {
            let publisher = publisher.clone();
            let op = operation.clone();
            tokio::spawn(async move {
                if let Err(e) = publisher.publish_operation_created(&op).await {
                    tracing::warn!(
                        operation_id = %op.id,
                        error = %e,
                        "Import Sync Hook: Kafka publish failed (non-fatal)"
                    );
                }
            });
        }

        Ok(())
    }
}

/// Parse une URL GitHub en (owner, repo).
///
/// Supporte :
/// - `https://github.com/owner/repo`
/// - `https://github.com/owner/repo.git`
/// - `github.com/owner/repo`
/// - `owner/repo` (shorthand)
fn parse_github_url(url: &str) -> Result<(String, String), DomainError> {
    let cleaned = url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git");

    // Supprimer le protocole et le host
    let path = if cleaned.contains("github.com") {
        cleaned
            .split("github.com")
            .last()
            .unwrap_or("")
            .trim_start_matches('/')
    } else {
        cleaned
    };

    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.len() >= 2 {
        Ok((parts[0].to_string(), parts[1].to_string()))
    } else {
        Err(DomainError::BusinessRule(format!(
            "URL GitHub invalide: '{url}'. Format attendu: https://github.com/owner/repo"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url_full() {
        let (owner, repo) = parse_github_url("https://github.com/tokio-rs/tokio").unwrap();
        assert_eq!(owner, "tokio-rs");
        assert_eq!(repo, "tokio");
    }

    #[test]
    fn test_parse_github_url_with_git_suffix() {
        let (owner, repo) =
            parse_github_url("https://github.com/BurntSushi/ripgrep.git").unwrap();
        assert_eq!(owner, "BurntSushi");
        assert_eq!(repo, "ripgrep");
    }

    #[test]
    fn test_parse_github_url_shorthand() {
        let (owner, repo) = parse_github_url("tokio-rs/tokio").unwrap();
        assert_eq!(owner, "tokio-rs");
        assert_eq!(repo, "tokio");
    }

    #[test]
    fn test_parse_github_url_no_protocol() {
        let (owner, repo) = parse_github_url("github.com/rust-lang/rust").unwrap();
        assert_eq!(owner, "rust-lang");
        assert_eq!(repo, "rust");
    }

    #[test]
    fn test_parse_github_url_trailing_slash() {
        let (owner, repo) = parse_github_url("https://github.com/user/repo/").unwrap();
        assert_eq!(owner, "user");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_url_invalid() {
        assert!(parse_github_url("not-a-url").is_err());
    }
}
