//! Use Case: ResolveRepo — Résolution sémantique des dépôts.
//!
//! Résout un couple `(owner_handle, repo_name)` en une entité `Repository`.
//! C'est le cœur du routage fédéré `/{owner}/{repo}/...` (Phase 10C).
//!
//! ## Flow
//! 1. `ActorRepository.find_by_handle(owner_handle)` → `Actor`
//! 2. `RepoRepository.find_by_owner_and_name(actor.id, repo_name)` → `Repository`
//!
//! ## Erreurs
//! - `NotFound` si le owner n'existe pas
//! - `NotFound` si le repo n'existe pas pour ce owner

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::entities::repository::Repository;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::repo_repository::RepoRepository;

/// Use case de résolution sémantique des dépôts.
///
/// Transforme les identifiants humains `(owner_handle, repo_name)`
/// en `Repository` avec son UUID, permettant au routeur fédéré
/// de déléguer aux use cases existants avec le bon `repository_id`.
pub struct ResolveRepoUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl ResolveRepoUseCase {
    /// Construit le use case avec les repositories injectés.
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self {
            actor_repo,
            repo_repo,
        }
    }

    /// Résout `(owner_handle, repo_name)` → `Repository`.
    ///
    /// # Errors
    /// - `DomainError::NotFound` si l'owner ou le repo n'existe pas.
    #[instrument(skip(self), fields(owner = %owner_handle, repo = %repo_name))]
    pub async fn execute(
        &self,
        owner_handle: &str,
        repo_name: &str,
    ) -> Result<Repository, DomainError> {
        // Étape 1 : Résoudre le owner par son handle
        let actor = self
            .actor_repo
            .find_by_handle(owner_handle)
            .await?
            .ok_or_else(|| {
                DomainError::NotFound {
                    entity_type: "Actor",
                    id: Uuid::nil(),
                }
            })?;

        // Étape 2 : Résoudre le repo par owner_id + name
        let repo = self
            .repo_repo
            .find_by_owner_and_name(&actor.id, repo_name)
            .await?
            .ok_or_else(|| {
                DomainError::NotFound {
                    entity_type: "Repository",
                    id: actor.id,
                }
            })?;

        Ok(repo)
    }

    /// Résolution directe par UUID (bypass le slug resolution).
    ///
    /// Utilisé quand le `repository_id` est déjà connu (ex: body/query param).
    #[instrument(skip(self))]
    pub async fn find_by_id(&self, repo_id: &Uuid) -> Result<Repository, DomainError> {
        self.repo_repo
            .find_by_id(repo_id)
            .await?
            .ok_or_else(|| {
                DomainError::NotFound {
                    entity_type: "Repository",
                    id: *repo_id,
                }
            })
    }
}

// ── Tests unitaires ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::entities::actor::{Actor, ActorType};
    use domain::entities::repository::{Repository, Visibility};

    // ── Mock ActorRepository ──────────────────────────
    struct MockActorRepo {
        actor: Option<Actor>,
    }

    #[async_trait]
    impl ActorRepository for MockActorRepo {
        async fn save(&self, _actor: &Actor) -> Result<(), DomainError> {
            Ok(())
        }
        async fn find_by_id(&self, _id: &Uuid) -> Result<Option<Actor>, DomainError> {
            Ok(self.actor.clone())
        }
        async fn find_by_handle(&self, handle: &str) -> Result<Option<Actor>, DomainError> {
            Ok(self
                .actor
                .as_ref()
                .filter(|a| a.handle == handle)
                .cloned())
        }
        async fn find_by_email(&self, _email: &str) -> Result<Option<Actor>, DomainError> {
            Ok(None)
        }
        async fn save_credential(&self, _actor_id: &Uuid, _cred_type: &str, _secret_hash: &str, _email: Option<&str>, _label: Option<&str>) -> Result<(), DomainError> {
            Ok(())
        }
        async fn find_credential_hash(&self, _actor_id: &Uuid, _cred_type: &str) -> Result<Option<String>, DomainError> {
            Ok(None)
        }
        async fn find_all_credential_hashes(&self, _actor_id: &Uuid, _cred_type: &str) -> Result<Vec<String>, DomainError> {
            Ok(vec![])
        }
        async fn find_actor_by_credential_hash(&self, _hash: &str, _cred_type: &str) -> Result<Option<Actor>, DomainError> {
            Ok(None)
        }
        async fn list_pats(&self, _actor_id: &Uuid) -> Result<Vec<domain::ports::actor_repository::PatInfo>, DomainError> {
            Ok(vec![])
        }
    }

    // ── Mock RepoRepository ──────────────────────────
    struct MockRepoRepo {
        repo: Option<Repository>,
    }

    #[async_trait]
    impl RepoRepository for MockRepoRepo {
        async fn save(&self, _repo: &Repository) -> Result<(), DomainError> {
            Ok(())
        }
        async fn find_by_id(&self, id: &Uuid) -> Result<Option<Repository>, DomainError> {
            Ok(self.repo.as_ref().filter(|r| r.id == *id).cloned())
        }
        async fn find_by_owner_and_name(
            &self,
            owner_id: &Uuid,
            name: &str,
        ) -> Result<Option<Repository>, DomainError> {
            Ok(self
                .repo
                .as_ref()
                .filter(|r| r.owner_id == *owner_id && r.name == name)
                .cloned())
        }
        async fn list_by_owner(&self, _owner_id: &Uuid) -> Result<Vec<Repository>, DomainError> {
            Ok(self.repo.clone().into_iter().collect())
        }
        async fn list_public(&self, _limit: usize) -> Result<Vec<Repository>, DomainError> {
            Ok(self.repo.clone().into_iter().collect())
        }
        async fn add_collaborator(
            &self,
            _actor_id: &Uuid,
            _repo_id: &Uuid,
            _role: &str,
        ) -> Result<(), DomainError> {
            Ok(())
        }
        async fn is_collaborator(&self, _actor_id: &Uuid, _repo_id: &Uuid) -> Result<bool, DomainError> {
            Ok(true)
        }
        async fn get_role(&self, _actor_id: &Uuid, _repo_id: &Uuid) -> Result<Option<String>, DomainError> {
            Ok(Some("owner".to_string()))
        }
    }

    // ── Helpers ──────────────────────────────────────
    fn make_actor() -> Actor {
        Actor::new("yusuf", "Yusuf", ActorType::Human)
    }

    fn make_repo(owner_id: Uuid) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id,
            name: "shinobi".to_string(),
            display_name: "Shinobi VCS".to_string(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_string(),
            created_at: chrono::Utc::now(),
        }
    }

    // ── Tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_resolve_nominal() {
        let actor = make_actor();
        let repo = make_repo(actor.id);

        let uc = ResolveRepoUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(actor.clone()),
            }),
            Arc::new(MockRepoRepo {
                repo: Some(repo.clone()),
            }),
        );

        let result = uc.execute("yusuf", "shinobi").await.unwrap();
        assert_eq!(result.id, repo.id);
        assert_eq!(result.name, "shinobi");
    }

    #[tokio::test]
    async fn test_resolve_owner_not_found() {
        let uc = ResolveRepoUseCase::new(
            Arc::new(MockActorRepo { actor: None }),
            Arc::new(MockRepoRepo { repo: None }),
        );

        let err = uc.execute("ghost", "shinobi").await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { entity_type: "Actor", .. }));
    }

    #[tokio::test]
    async fn test_resolve_repo_not_found() {
        let actor = make_actor();

        let uc = ResolveRepoUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(actor.clone()),
            }),
            Arc::new(MockRepoRepo { repo: None }),
        );

        let err = uc.execute("yusuf", "missing-repo").await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { entity_type: "Repository", .. }));
    }

    #[tokio::test]
    async fn test_resolve_by_id_nominal() {
        let actor = make_actor();
        let repo = make_repo(actor.id);
        let repo_id = repo.id;

        let uc = ResolveRepoUseCase::new(
            Arc::new(MockActorRepo { actor: None }),
            Arc::new(MockRepoRepo {
                repo: Some(repo.clone()),
            }),
        );

        let result = uc.find_by_id(&repo_id).await.unwrap();
        assert_eq!(result.id, repo_id);
    }

    #[tokio::test]
    async fn test_resolve_by_id_not_found() {
        let uc = ResolveRepoUseCase::new(
            Arc::new(MockActorRepo { actor: None }),
            Arc::new(MockRepoRepo { repo: None }),
        );

        let err = uc.find_by_id(&Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { entity_type: "Repository", .. }));
    }
}
