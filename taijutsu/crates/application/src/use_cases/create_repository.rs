//! Use Case: CreateRepository — Création d'un dépôt dans la Forge Sociale.
//!
//! Orchestre la création complète d'un dépôt multi-tenant :
//! 1. Vérification de l'existence du propriétaire (Actor)
//! 2. Validation du slug (nom du dépôt)
//! 3. Persistence dans PostgreSQL
//! 4. Initialisation du workspace VCS (Jujutsu)
//! 5. Ajout du propriétaire comme collaborateur Owner
//!
//! ## Erreurs
//! - `NotFound` si le owner n'existe pas
//! - `Duplicate` si le slug est déjà pris pour ce owner
//! - `BusinessRule` si le slug est invalide (caractères interdits)

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::repository::{Repository, Visibility};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::vcs_engine::VcsEngine;

// ── Commande ──────────────────────────────────────────────────────────

/// Commande de création d'un dépôt.
#[derive(Debug)]
pub struct CreateRepositoryCommand {
    /// UUID de l'acteur propriétaire du dépôt.
    pub owner_id: Uuid,
    /// Nom slug unique par owner (ex: "my-project").
    /// Utilisé dans les URLs : /{owner_handle}/{name}
    pub name: String,
    /// Nom d'affichage lisible (ex: "My Project").
    pub display_name: String,
    /// Description optionnelle.
    pub description: Option<String>,
    /// Visibilité du dépôt (défaut: Public).
    pub visibility: Visibility,
}

// ── Use Case ──────────────────────────────────────────────────────────

/// Use case de création de dépôt dans la Forge Sociale.
pub struct CreateRepositoryUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    vcs: Arc<dyn VcsEngine>,
}

impl CreateRepositoryUseCase {
    /// Construit le use case avec les adaptateurs injectés.
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        vcs: Arc<dyn VcsEngine>,
    ) -> Self {
        Self {
            actor_repo,
            repo_repo,
            vcs,
        }
    }

    /// Exécute la création complète du dépôt.
    ///
    /// ## Flow
    /// 1. Valider le slug (caractères autorisés)
    /// 2. Vérifier que le owner existe
    /// 3. Construire l'entité Repository
    /// 4. Persister en base (détection doublon via UNIQUE constraint)
    /// 5. Initialiser le workspace VCS
    /// 6. Ajouter le owner comme collaborateur Owner
    ///
    /// ## Errors
    /// - `DomainError::BusinessRule` si le slug est invalide
    /// - `DomainError::NotFound` si le owner n'existe pas
    /// - `DomainError::Duplicate` si le couple (owner, name) existe déjà
    #[instrument(skip(self), fields(owner_id = %cmd.owner_id, name = %cmd.name))]
    pub async fn execute(&self, cmd: CreateRepositoryCommand) -> Result<Repository, DomainError> {
        // Étape 1 : Validation du slug
        validate_slug(&cmd.name)?;

        // Étape 2 : Vérifier que le owner existe
        let _actor = self
            .actor_repo
            .find_by_id(&cmd.owner_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: cmd.owner_id,
            })?;

        // Étape 3 : Construire l'entité
        let mut repo = Repository::new(cmd.owner_id, &cmd.name, &cmd.display_name);
        repo.description = cmd.description;
        repo.visibility = cmd.visibility;

        // Étape 4 : Persister (le port gère la détection de doublon)
        self.repo_repo.save(&repo).await?;

        info!(
            repo_id = %repo.id,
            name = %repo.name,
            owner_id = %repo.owner_id,
            "✅ Dépôt créé dans PostgreSQL"
        );

        // Étape 5 : Initialiser le workspace VCS (Phase 21: owner_id/repo_id)
        self.vcs.init_workspace(&cmd.owner_id, &repo.id).await?;

        info!(
            repo_id = %repo.id,
            "✅ Workspace VCS initialisé"
        );

        // Étape 6 : Ajouter le owner comme collaborateur Owner
        self.repo_repo
            .add_collaborator(&cmd.owner_id, &repo.id, "owner")
            .await?;

        info!(
            repo_id = %repo.id,
            owner_id = %cmd.owner_id,
            "✅ Collaborateur Owner ajouté — Dépôt opérationnel"
        );

        Ok(repo)
    }
}

// ── Validation du slug ────────────────────────────────────────────────

/// Valide un slug de dépôt.
///
/// Règles :
/// - 1 à 64 caractères
/// - Lettres minuscules, chiffres, tirets, underscores
/// - Ne commence ni ne finit par un tiret
/// - Pas de tirets consécutifs
fn validate_slug(name: &str) -> Result<(), DomainError> {
    if name.is_empty() || name.len() > 64 {
        return Err(DomainError::BusinessRule(
            "Le nom du dépôt doit contenir entre 1 et 64 caractères".to_string(),
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(DomainError::BusinessRule(
            "Le nom du dépôt ne peut contenir que des lettres minuscules, chiffres, tirets et underscores".to_string(),
        ));
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(DomainError::BusinessRule(
            "Le nom du dépôt ne peut pas commencer ou finir par un tiret".to_string(),
        ));
    }

    if name.contains("--") {
        return Err(DomainError::BusinessRule(
            "Le nom du dépôt ne peut pas contenir de tirets consécutifs".to_string(),
        ));
    }

    Ok(())
}

// ── Tests unitaires ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::entities::actor::{Actor, ActorType};
    use domain::entities::content_id::ContentId;
    use std::sync::atomic::{AtomicBool, Ordering};

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
        async fn find_by_handle(&self, _handle: &str) -> Result<Option<Actor>, DomainError> {
            Ok(self.actor.clone())
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
        async fn find_by_github_id(&self, _github_id: i64) -> Result<Option<Actor>, DomainError> {
            Ok(None)
        }
        async fn update_github_id(&self, _actor_id: &Uuid, _github_id: i64) -> Result<(), DomainError> {
            Ok(())
        }
        async fn update_github_token(&self, _actor_id: &Uuid, _token: &str) -> Result<(), DomainError> {
            Ok(())
        }
        async fn get_github_token(&self, _actor_id: &Uuid) -> Result<Option<String>, DomainError> {
            Ok(None)
        }
        async fn list_pats(&self, _actor_id: &Uuid) -> Result<Vec<domain::ports::actor_repository::PatInfo>, DomainError> {
            Ok(vec![])
        }
    }

    // ── Mock RepoRepository ──────────────────────────
    struct MockRepoRepo {
        fail_duplicate: bool,
        saved: AtomicBool,
        collaborator_added: AtomicBool,
    }

    impl MockRepoRepo {
        fn new() -> Self {
            Self {
                fail_duplicate: false,
                saved: AtomicBool::new(false),
                collaborator_added: AtomicBool::new(false),
            }
        }

        fn with_duplicate() -> Self {
            Self {
                fail_duplicate: true,
                saved: AtomicBool::new(false),
                collaborator_added: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl RepoRepository for MockRepoRepo {
        async fn save(&self, _repo: &Repository) -> Result<(), DomainError> {
            if self.fail_duplicate {
                return Err(DomainError::Duplicate("Dépôt existe déjà".to_string()));
            }
            self.saved.store(true, Ordering::SeqCst);
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
        async fn list_by_owner(&self, _owner_id: &Uuid) -> Result<Vec<Repository>, DomainError> {
            Ok(vec![])
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
            self.collaborator_added.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn is_collaborator(&self, _actor_id: &Uuid, _repo_id: &Uuid) -> Result<bool, DomainError> {
            Ok(true)
        }
        async fn get_role(&self, _actor_id: &Uuid, _repo_id: &Uuid) -> Result<Option<String>, DomainError> {
            Ok(Some("owner".to_string()))
        }
        async fn update_mirror_synced_at(&self, _repo_id: &Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    // ── Mock VcsEngine ──────────────────────────────
    struct MockVcsEngine {
        initialized: AtomicBool,
    }

    impl MockVcsEngine {
        fn new() -> Self {
            Self {
                initialized: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl VcsEngine for MockVcsEngine {
        async fn init_workspace(&self, _owner_id: &Uuid, _repo_id: &Uuid) -> Result<(), DomainError> {
            self.initialized.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn create_operation(
            &self,
            _repo_id: &Uuid,
            _description: &str,
            _parent_ids: &[String],
            _files: &[(String, Vec<u8>)],
        ) -> Result<ContentId, DomainError> {
            Ok(ContentId::new("mock-content-id"))
        }
        async fn resolve_head(&self, _repo_id: &Uuid) -> Result<Option<ContentId>, DomainError> {
            Ok(None)
        }
        async fn diff_since(&self, _repo_id: &Uuid, _content_id: &ContentId) -> Result<Vec<String>, DomainError> {
            Ok(vec![])
        }
        async fn list_tree(&self, _repo_id: &Uuid, _revision: &str, _path: &str) -> Result<Vec<domain::ports::vcs_engine::TreeEntry>, DomainError> {
            Ok(vec![])
        }
        async fn read_blob(&self, _repo_id: &Uuid, _revision: &str, _path: &str) -> Result<Vec<u8>, DomainError> {
            Ok(vec![])
        }
        async fn list_refs(&self, _repo_id: &Uuid) -> Result<Vec<domain::ports::vcs_engine::RefInfo>, DomainError> {
            Ok(vec![])
        }
        async fn diff_content(&self, _repo_id: &Uuid, _cid: &ContentId) -> Result<Vec<domain::ports::vcs_engine::FileDiff>, DomainError> {
            Ok(vec![])
        }
    }

    // ── Helpers ──────────────────────────────────────
    fn make_actor() -> Actor {
        Actor::new("alice", "Alice Dev", ActorType::Human)
    }

    fn make_cmd(owner_id: Uuid) -> CreateRepositoryCommand {
        CreateRepositoryCommand {
            owner_id,
            name: "my-project".to_string(),
            display_name: "My Project".to_string(),
            description: Some("A test project".to_string()),
            visibility: Visibility::Public,
        }
    }

    // ── Tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_create_repository_nominal() {
        let actor = make_actor();
        let owner_id = actor.id;
        let repo_repo = Arc::new(MockRepoRepo::new());
        let vcs = Arc::new(MockVcsEngine::new());

        let uc = CreateRepositoryUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(actor),
            }),
            repo_repo.clone(),
            vcs.clone(),
        );

        let result = uc.execute(make_cmd(owner_id)).await.unwrap();

        assert_eq!(result.name, "my-project");
        assert_eq!(result.display_name, "My Project");
        assert_eq!(result.owner_id, owner_id);
        assert_eq!(result.visibility, Visibility::Public);
        assert!(repo_repo.saved.load(Ordering::SeqCst));
        assert!(repo_repo.collaborator_added.load(Ordering::SeqCst));
        assert!(vcs.initialized.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_create_repository_owner_not_found() {
        let uc = CreateRepositoryUseCase::new(
            Arc::new(MockActorRepo { actor: None }),
            Arc::new(MockRepoRepo::new()),
            Arc::new(MockVcsEngine::new()),
        );

        let err = uc.execute(make_cmd(Uuid::new_v4())).await.unwrap_err();
        assert!(matches!(
            err,
            DomainError::NotFound {
                entity_type: "Actor",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn test_create_repository_duplicate() {
        let actor = make_actor();
        let owner_id = actor.id;

        let uc = CreateRepositoryUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(actor),
            }),
            Arc::new(MockRepoRepo::with_duplicate()),
            Arc::new(MockVcsEngine::new()),
        );

        let err = uc.execute(make_cmd(owner_id)).await.unwrap_err();
        assert!(matches!(err, DomainError::Duplicate(_)));
    }

    #[tokio::test]
    async fn test_create_repository_invalid_slugs() {
        let actor = make_actor();
        let owner_id = actor.id;

        let uc = CreateRepositoryUseCase::new(
            Arc::new(MockActorRepo {
                actor: Some(actor),
            }),
            Arc::new(MockRepoRepo::new()),
            Arc::new(MockVcsEngine::new()),
        );

        // Slug vide
        let mut cmd = make_cmd(owner_id);
        cmd.name = "".to_string();
        assert!(matches!(
            uc.execute(cmd).await.unwrap_err(),
            DomainError::BusinessRule(_)
        ));

        // Caractères interdits (majuscules)
        let mut cmd = make_cmd(owner_id);
        cmd.name = "My-Project".to_string();
        assert!(matches!(
            uc.execute(cmd).await.unwrap_err(),
            DomainError::BusinessRule(_)
        ));

        // Commence par un tiret
        let mut cmd = make_cmd(owner_id);
        cmd.name = "-bad-name".to_string();
        assert!(matches!(
            uc.execute(cmd).await.unwrap_err(),
            DomainError::BusinessRule(_)
        ));

        // Tirets consécutifs
        let mut cmd = make_cmd(owner_id);
        cmd.name = "bad--name".to_string();
        assert!(matches!(
            uc.execute(cmd).await.unwrap_err(),
            DomainError::BusinessRule(_)
        ));

        // Trop long (65 caractères)
        let mut cmd = make_cmd(owner_id);
        cmd.name = "a".repeat(65);
        assert!(matches!(
            uc.execute(cmd).await.unwrap_err(),
            DomainError::BusinessRule(_)
        ));
    }
}
