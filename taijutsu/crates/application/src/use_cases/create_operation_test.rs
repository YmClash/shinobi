//! Tests unitaires pour CreateOperationUseCase.
//!
//! Utilise des mocks manuels (pas de framework) pour tester les trois flux :
//! - Nominal : VCS ok + IPFS ok → Operation avec 2 CID
//! - Dégradé : VCS ok + IPFS absent → Operation avec 1 CID
//! - Erreur : VCS error → DomainError::VcsError

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use domain::entities::content_id::ContentId;
    use domain::entities::operation::Operation;
    use domain::errors::DomainError;
    use domain::ports::content_store::ContentStore;
    use domain::ports::event_publisher::{AnalysisCompleteSummary, EventPublisher};
    use domain::ports::repository::OperationRepository;
    use domain::ports::vcs_engine::VcsEngine;

    use crate::use_cases::create_operation::{CreateOperationCommand, CreateOperationUseCase};

    // ── Mock VcsEngine ────────────────────────────────────

    struct MockVcsEngine {
        /// CID à retourner par create_operation
        cid: String,
        /// Si Some, create_operation retourne cette erreur
        error: Option<String>,
    }

    impl MockVcsEngine {
        fn ok(cid: &str) -> Self {
            Self {
                cid: cid.to_string(),
                error: None,
            }
        }

        fn failing(error: &str) -> Self {
            Self {
                cid: String::new(),
                error: Some(error.to_string()),
            }
        }
    }

    #[async_trait]
    impl VcsEngine for MockVcsEngine {
        async fn init_workspace(&self, _owner_id: &Uuid, _repo_id: &Uuid) -> Result<(), DomainError> {
            Ok(())
        }

        async fn create_operation(
            &self,
            _repo_id: &Uuid,
            _description: &str,
            _parent_ids: &[String],
            _files: &[(String, Vec<u8>)],
        ) -> Result<ContentId, DomainError> {
            if let Some(ref err) = self.error {
                Err(DomainError::VcsError(err.clone()))
            } else {
                Ok(ContentId::new(&self.cid))
            }
        }

        async fn resolve_head(&self, _repo_id: &Uuid) -> Result<Option<ContentId>, DomainError> {
            Ok(None)
        }

        async fn diff_since(&self, _repo_id: &Uuid, _cid: &ContentId) -> Result<Vec<String>, DomainError> {
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
        async fn can_fast_forward(&self, _repo_id: &Uuid, _source: &str, _target: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
        async fn merge_fast_forward(&self, _repo_id: &Uuid, _source: &str, _target: &str) -> Result<ContentId, DomainError> {
            Ok(ContentId::new("mock-merge-commit"))
        }
        async fn squash_merge(&self, _repo_id: &Uuid, _source: &str, _target: &str, _message: &str) -> Result<ContentId, DomainError> {
            Ok(ContentId::new("mock-squash-commit"))
        }
        async fn diff_merge_base(&self, _repo_id: &Uuid, _source: &str, _target: &str) -> Result<Vec<domain::ports::vcs_engine::FileDiff>, DomainError> {
            Ok(vec![])
        }
        async fn clone_workspace(&self, _source_owner_id: &Uuid, _source_repo_id: &Uuid, _target_owner_id: &Uuid, _target_repo_id: &Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    // ── Mock OperationRepository ──────────────────────────

    struct MockRepository {
        saved: Mutex<Vec<Operation>>,
    }

    impl MockRepository {
        fn new() -> Self {
            Self {
                saved: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl OperationRepository for MockRepository {
        async fn save(&self, operation: &Operation) -> Result<(), DomainError> {
            self.saved.lock().await.push(operation.clone());
            Ok(())
        }

        async fn find_by_id(&self, _id: &Uuid) -> Result<Option<Operation>, DomainError> {
            Ok(None)
        }

        async fn find_by_content_id(&self, _repo_id: &Uuid, _content_id: &str) -> Result<Option<Operation>, DomainError> {
            Ok(None)
        }

        async fn list_recent(&self, _repo_id: &Uuid, _limit: usize) -> Result<Vec<Operation>, DomainError> {
            Ok(vec![])
        }

        async fn find_by_author(&self, _author_id: &Uuid) -> Result<Vec<Operation>, DomainError> {
            Ok(vec![])
        }
        async fn count_by_repo(&self, _repo_id: &Uuid) -> Result<i64, DomainError> {
            Ok(0)
        }
    }

    // ── Mock ContentStore (IPFS) ─────────────────────────

    struct MockContentStore {
        /// CID à retourner par store() et store_dag()
        cid: String,
        /// Compteur d'appels à store()
        store_count: Mutex<usize>,
        /// Compteur d'appels à store_dag() (Phase 8.1)
        dag_store_count: Mutex<usize>,
    }

    impl MockContentStore {
        fn new(cid: &str) -> Self {
            Self {
                cid: cid.to_string(),
                store_count: Mutex::new(0),
                dag_store_count: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl ContentStore for MockContentStore {
        async fn store(&self, _data: &[u8]) -> Result<ContentId, DomainError> {
            *self.store_count.lock().await += 1;
            Ok(ContentId::new(&self.cid))
        }

        async fn retrieve(&self, _cid: &ContentId) -> Result<Vec<u8>, DomainError> {
            Ok(vec![])
        }

        async fn exists(&self, _cid: &ContentId) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn pin(&self, _cid: &ContentId) -> Result<(), DomainError> {
            Ok(())
        }

        async fn store_dag(
            &self,
            description: &str,
            files: &[(String, Vec<u8>)],
        ) -> Result<(ContentId, domain::ports::content_store::DagManifest), DomainError> {
            *self.dag_store_count.lock().await += 1;
            let manifest = domain::ports::content_store::DagManifest {
                version: 1,
                description: description.to_string(),
                files: files.iter().map(|(path, content)| {
                    domain::ports::content_store::DagFileLink {
                        path: path.clone(),
                        cid: format!("QmFile_{}", path.replace('/', "_")),
                        size: content.len(),
                    }
                }).collect(),
            };
            Ok((ContentId::new(&self.cid), manifest))
        }
    }

    // ── Mock EventPublisher ──────────────────────────────

    struct MockEventPublisher {
        published: Mutex<Vec<Uuid>>,
    }

    impl MockEventPublisher {
        fn new() -> Self {
            Self {
                published: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl EventPublisher for MockEventPublisher {
        async fn publish_operation_created(
            &self,
            operation: &Operation,
        ) -> Result<(), DomainError> {
            self.published.lock().await.push(operation.id);
            Ok(())
        }

        async fn publish_analysis_complete(
            &self,
            _summary: &AnalysisCompleteSummary,
        ) -> Result<(), DomainError> {
            Ok(())
        }
    }

    // ── Helpers ──────────────────────────────────────────

    fn test_command() -> CreateOperationCommand {
        CreateOperationCommand {
            author_id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            description: "Test operation".to_string(),
            parent_ids: vec![],
            files: vec![],
        }
    }

    fn test_command_with_files() -> CreateOperationCommand {
        CreateOperationCommand {
            author_id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            description: "Operation with files".to_string(),
            parent_ids: vec![],
            files: vec![
                ("README.md".to_string(), b"# Hello".to_vec()),
                ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
            ],
        }
    }

    // ── Tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_nominal_flow_vcs_only_no_files() {
        // Sans fichiers + sans ContentStore → juste le CID jj-lib
        let vcs = Arc::new(MockVcsEngine::ok("QmJjCidNominal"));
        let repo = Arc::new(MockRepository::new());

        let use_case = CreateOperationUseCase::new(
            vcs, repo.clone(), None, None,
        );

        let result = use_case.execute(test_command()).await;
        assert!(result.is_ok());

        let op = result.unwrap().operation;
        assert_eq!(op.content_id.as_str(), "QmJjCidNominal");
        assert!(!op.has_ipfs_content(), "Sans ContentStore → pas d'IPFS CID");

        // Vérifie la persistance
        let saved = repo.saved.lock().await;
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, op.id);
    }

    #[tokio::test]
    async fn test_nominal_flow_with_ipfs_sync() {
        // Avec fichiers + ContentStore → CID jj-lib + CID IPFS
        let vcs = Arc::new(MockVcsEngine::ok("QmJjCidSync"));
        let repo = Arc::new(MockRepository::new());
        let store = Arc::new(MockContentStore::new("QmIpfsCidSync"));

        let use_case = CreateOperationUseCase::new(
            vcs, repo.clone(), None, Some(store.clone()),
        );

        let result = use_case.execute(test_command_with_files()).await;
        assert!(result.is_ok());

        let op = result.unwrap().operation;
        assert_eq!(op.content_id.as_str(), "QmJjCidSync");
        assert!(op.has_ipfs_content(), "Avec fichiers + ContentStore → IPFS CID présent");
        assert_eq!(op.ipfs_content_id.as_ref().unwrap().as_str(), "QmIpfsCidSync");

        // ContentStore.store_dag() doit avoir été appelé (Phase 8.1)
        let count = *store.dag_store_count.lock().await;
        assert_eq!(count, 1, "store_dag() devrait être appelé une fois");
    }

    #[tokio::test]
    async fn test_degraded_flow_no_content_store() {
        // ContentStore absent → opération réussit sans IPFS CID
        let vcs = Arc::new(MockVcsEngine::ok("QmJjCidDegraded"));
        let repo = Arc::new(MockRepository::new());

        let use_case = CreateOperationUseCase::new(
            vcs, repo.clone(), None, None,
        );

        let result = use_case.execute(test_command_with_files()).await;
        assert!(result.is_ok());

        let op = result.unwrap().operation;
        assert_eq!(op.content_id.as_str(), "QmJjCidDegraded");
        assert!(!op.has_ipfs_content());
    }

    #[tokio::test]
    async fn test_no_ipfs_sync_when_files_empty() {
        // ContentStore présent mais pas de fichiers → pas de sync IPFS
        let vcs = Arc::new(MockVcsEngine::ok("QmJjCidEmpty"));
        let repo = Arc::new(MockRepository::new());
        let store = Arc::new(MockContentStore::new("QmShouldNotBeCalled"));

        let use_case = CreateOperationUseCase::new(
            vcs, repo.clone(), None, Some(store.clone()),
        );

        let result = use_case.execute(test_command()).await;
        assert!(result.is_ok());

        let op = result.unwrap().operation;
        assert!(!op.has_ipfs_content(), "Pas de fichiers → pas de sync IPFS");

        let count = *store.dag_store_count.lock().await;
        assert_eq!(count, 0, "store_dag() ne devrait PAS être appelé sans fichiers");
    }

    #[tokio::test]
    async fn test_vcs_error_propagates() {
        // VcsEngine retourne une erreur → le use case échoue proprement
        let vcs = Arc::new(MockVcsEngine::failing("workspace corrupted"));
        let repo = Arc::new(MockRepository::new());

        let use_case = CreateOperationUseCase::new(
            vcs, repo.clone(), None, None,
        );

        let result = use_case.execute(test_command()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            DomainError::VcsError(msg) => {
                assert!(msg.contains("workspace corrupted"));
            }
            other => panic!("Expected VcsError, got: {other:?}"),
        }

        // Rien ne doit être persisté
        let saved = repo.saved.lock().await;
        assert!(saved.is_empty(), "Rien ne doit être sauvegardé si le VCS échoue");
    }

    #[tokio::test]
    async fn test_event_published_on_success() {
        // Kafka publisher est appelé après la persistance
        let vcs = Arc::new(MockVcsEngine::ok("QmJjEvent"));
        let repo = Arc::new(MockRepository::new());
        let publisher = Arc::new(MockEventPublisher::new());

        let use_case = CreateOperationUseCase::new(
            vcs, repo.clone(), Some(publisher.clone()), None,
        );

        let result = use_case.execute(test_command()).await;
        assert!(result.is_ok());
        let op = result.unwrap().operation;

        // tokio::spawn est fire-and-forget, attendons un moment
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let published = publisher.published.lock().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0], op.id);
    }
}
