//! Use Case: UpdateRepoSettings — Réglages d'un dépôt (Phase 42-pre-Fix).
//!
//! V2 minimale : opt-in de confidentialité IA `allow_external_llm`.
//! **Réservé au owner** (un maintainer ne peut pas décider d'envoyer le
//! code du dépôt à un tiers).

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::repo_repository::RepoRepository;

pub struct UpdateRepoSettingsUseCase {
    repo_repo: Arc<dyn RepoRepository>,
}

impl UpdateRepoSettingsUseCase {
    pub fn new(repo_repo: Arc<dyn RepoRepository>) -> Self {
        Self { repo_repo }
    }

    /// Active/désactive l'envoi du code à un LLM externe.
    ///
    /// # Errors
    /// - `Forbidden` : l'acteur n'est pas owner du dépôt.
    /// - `NotFound` : dépôt inexistant.
    #[instrument(skip(self))]
    pub async fn set_allow_external_llm(
        &self,
        actor_id: &Uuid,
        repository_id: &Uuid,
        allow: bool,
    ) -> Result<(), DomainError> {
        let role = self.repo_repo.get_role(actor_id, repository_id).await?;
        if role.as_deref() != Some("owner") {
            return Err(DomainError::Forbidden(
                "Seul le propriétaire peut modifier la politique de confidentialité IA".to_string(),
            ));
        }

        if !self.repo_repo.update_llm_policy(repository_id, allow).await? {
            return Err(DomainError::NotFound {
                entity_type: "Repository",
                id: *repository_id,
            });
        }

        info!(
            actor_id = %actor_id,
            repository_id = %repository_id,
            allow_external_llm = allow,
            "🔒 Politique de confidentialité IA mise à jour"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::entities::repository::Repository;

    /// Mock minimal : seuls `get_role` et `update_llm_policy` sont utilisés.
    struct MockRepoRepo {
        role: Option<String>,
        calls: Mutex<Vec<bool>>,
    }

    #[async_trait]
    impl RepoRepository for MockRepoRepo {
        async fn save(&self, _: &Repository) -> Result<(), DomainError> { unimplemented!() }
        async fn find_by_id(&self, _: &Uuid) -> Result<Option<Repository>, DomainError> { unimplemented!() }
        async fn find_by_owner_and_name(&self, _: &Uuid, _: &str) -> Result<Option<Repository>, DomainError> { unimplemented!() }
        async fn list_by_owner(&self, _: &Uuid) -> Result<Vec<Repository>, DomainError> { unimplemented!() }
        async fn list_public(&self, _: usize) -> Result<Vec<Repository>, DomainError> { unimplemented!() }
        async fn add_collaborator(&self, _: &Uuid, _: &Uuid, _: &str) -> Result<(), DomainError> { unimplemented!() }
        async fn is_collaborator(&self, _: &Uuid, _: &Uuid) -> Result<bool, DomainError> { unimplemented!() }
        async fn get_role(&self, _: &Uuid, _: &Uuid) -> Result<Option<String>, DomainError> {
            Ok(self.role.clone())
        }
        async fn update_mirror_synced_at(&self, _: &Uuid) -> Result<(), DomainError> { unimplemented!() }
        async fn soft_delete(&self, _: &Uuid) -> Result<bool, DomainError> { unimplemented!() }
        async fn restore(&self, _: &Uuid) -> Result<bool, DomainError> { unimplemented!() }
        async fn hard_delete(&self, _: &Uuid) -> Result<bool, DomainError> { unimplemented!() }
        async fn list_deleted_by_owner(&self, _: &Uuid) -> Result<Vec<Repository>, DomainError> { unimplemented!() }
        async fn list_expired_trash(&self, _: i64) -> Result<Vec<Repository>, DomainError> { unimplemented!() }
        async fn count_forks(&self, _: &Uuid) -> Result<u64, DomainError> { unimplemented!() }
        async fn find_fork_by_owner(&self, _: &Uuid, _: &Uuid) -> Result<Option<Repository>, DomainError> { unimplemented!() }
        async fn update_llm_policy(&self, _: &Uuid, allow: bool) -> Result<bool, DomainError> {
            self.calls.lock().unwrap().push(allow);
            Ok(true)
        }
    }

    fn uc(role: Option<&str>) -> (UpdateRepoSettingsUseCase, Arc<MockRepoRepo>) {
        let repo = Arc::new(MockRepoRepo { role: role.map(String::from), calls: Mutex::new(vec![]) });
        (UpdateRepoSettingsUseCase::new(repo.clone()), repo)
    }

    #[tokio::test]
    async fn owner_can_toggle() {
        let (uc, repo) = uc(Some("owner"));
        uc.set_allow_external_llm(&Uuid::new_v4(), &Uuid::new_v4(), true).await.unwrap();
        assert_eq!(*repo.calls.lock().unwrap(), vec![true]);
    }

    #[tokio::test]
    async fn maintainer_is_forbidden() {
        let (uc, repo) = uc(Some("maintainer"));
        let err = uc.set_allow_external_llm(&Uuid::new_v4(), &Uuid::new_v4(), true).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden(_)));
        assert!(repo.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn non_collaborator_is_forbidden() {
        let (uc, _) = uc(None);
        let err = uc.set_allow_external_llm(&Uuid::new_v4(), &Uuid::new_v4(), false).await.unwrap_err();
        assert!(matches!(err, DomainError::Forbidden(_)));
    }
}
