//! Use Case : Lister les dépôts d'un acteur.
//!
//! Résout le handle de l'acteur → UUID, puis interroge
//! `RepoRepository.list_by_owner()` pour obtenir ses dépôts.

use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

use domain::entities::repository::Repository;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::repo_repository::RepoRepository;

// ── Use Case ──────────────────────────────────────────────────────────

/// Liste les dépôts d'un acteur identifié par son handle.
pub struct ListRepositoriesUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl ListRepositoriesUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self {
            actor_repo,
            repo_repo,
        }
    }

    /// Résout le handle → Actor UUID, puis liste ses dépôts.
    #[instrument(skip(self), fields(handle = %handle))]
    pub async fn execute(&self, handle: &str) -> Result<Vec<Repository>, DomainError> {
        // Étape 1 : Résoudre le handle en Actor
        let actor = self
            .actor_repo
            .find_by_handle(handle)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        // Étape 2 : Lister les dépôts de cet acteur
        self.repo_repo.list_by_owner(&actor.id).await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::entities::actor::{Actor, ActorType};

    // ── Mock ActorRepository ──────────────────────────
    struct MockActorRepo {
        actor: Option<Actor>,
    }

    #[async_trait]
    impl ActorRepository for MockActorRepo {
        async fn save(&self, _actor: &Actor) -> Result<(), DomainError> {
            Ok(())
        }
        async fn find_by_id(&self, id: &Uuid) -> Result<Option<Actor>, DomainError> {
            Ok(self.actor.as_ref().filter(|a| a.id == *id).cloned())
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
        repos: Vec<Repository>,
    }

    #[async_trait]
    impl RepoRepository for MockRepoRepo {
        async fn save(&self, _repo: &Repository) -> Result<(), DomainError> {
            Ok(())
        }
        async fn find_by_id(&self, _id: &Uuid) -> Result<Option<Repository>, DomainError> {
            Ok(None)
        }
        async fn find_by_owner_and_name(
            &self,
            _owner_id: &Uuid,
            _name: &str,
        ) -> Result<Option<Repository>, DomainError> {
            Ok(None)
        }
        async fn list_by_owner(&self, owner_id: &Uuid) -> Result<Vec<Repository>, DomainError> {
            Ok(self
                .repos
                .iter()
                .filter(|r| r.owner_id == *owner_id)
                .cloned()
                .collect())
        }
        async fn list_public(&self, _limit: usize) -> Result<Vec<Repository>, DomainError> {
            Ok(vec![])
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

    // ── Helpers ──────────────────────────
    fn make_actor(id: Uuid, handle: &str) -> Actor {
        Actor {
            id,
            handle: handle.to_string(),
            display_name: handle.to_string(),
            actor_type: ActorType::Human,
            avatar_url: None,
            email: None,
            bio: None,
            created_at: chrono::Utc::now(),
        }
    }

    fn make_repo(owner_id: Uuid, name: &str) -> Repository {
        Repository::new(owner_id, name.to_string(), name.to_string())
    }

    #[tokio::test]
    async fn test_list_repos_nominal() {
        let actor_id = Uuid::new_v4();
        let repo1 = make_repo(actor_id, "alpha");
        let repo2 = make_repo(actor_id, "beta");

        let uc = ListRepositoriesUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(make_actor(actor_id, "ymc")),
            }),
            Arc::new(MockRepoRepo {
                repos: vec![repo1, repo2],
            }),
        );

        let result = uc.execute("ymc").await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_list_repos_actor_not_found() {
        let uc = ListRepositoriesUseCase::new(
            Arc::new(MockActorRepo { actor: None }),
            Arc::new(MockRepoRepo { repos: vec![] }),
        );

        let err = uc.execute("ghost").await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { .. }));
    }

    #[tokio::test]
    async fn test_list_repos_empty() {
        let actor_id = Uuid::new_v4();

        let uc = ListRepositoriesUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(make_actor(actor_id, "ymc")),
            }),
            Arc::new(MockRepoRepo { repos: vec![] }),
        );

        let result = uc.execute("ymc").await.unwrap();
        assert!(result.is_empty());
    }
}
