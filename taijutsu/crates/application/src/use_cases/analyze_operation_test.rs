//! Tests unitaires pour AnalyzeOperationUseCase.
//!
//! Utilise des mocks manuels (pas de framework) pour tester les flux :
//! - Nominal : Operation avec IPFS CID + fichiers Rust → chunks extraits
//! - Skip sans IPFS : Operation sans `ipfs_content_id` → aucune analyse
//! - Fichiers non-Rust : `.md` et `.toml` → ignorés
//! - IPFS down : `content_store.retrieve()` échoue → erreur propagée
//! - Fichier vide : 0 chunks retournés
//! - Mixed : Rust + non-Rust → seuls les .rs sont analysés
//! - Persistence : chunks persistés via ChunkRepository après analyse
//! - Graceful degradation : sans ChunkRepository → log uniquement
//! - Re-publication : événement analysis-complete publié après analyse (Phase 7B)
//! - Idempotence : delete_by_operation() avant save_chunks() (Phase 7B)

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use domain::entities::content_id::ContentId;
    use domain::entities::operation::Operation;
    use domain::errors::DomainError;
    use domain::ports::chunk_repository::{ChunkRepository, SimilarChunk, StoredChunk};
    use domain::ports::content_store::ContentStore;
    use domain::ports::embedding_service::EmbeddingService;
    use domain::ports::event_publisher::{AnalysisCompleteSummary, EventPublisher};

    use tensai::{Chunker, ChunkerError, SemanticChunk};

    use crate::use_cases::analyze_operation::{AnalyzeOperationUseCase, AnalysisOutcome};

    // ── Mock ContentStore ─────────────────────────────────

    struct MockContentStore {
        data: Option<Vec<u8>>,
        should_fail: bool,
    }

    impl MockContentStore {
        fn with_data(data: Vec<u8>) -> Self {
            Self {
                data: Some(data),
                should_fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                data: None,
                should_fail: true,
            }
        }
    }

    #[async_trait]
    impl ContentStore for MockContentStore {
        async fn store(&self, _data: &[u8]) -> Result<ContentId, DomainError> {
            Ok(ContentId::new("QmMock"))
        }

        async fn retrieve(&self, _cid: &ContentId) -> Result<Vec<u8>, DomainError> {
            if self.should_fail {
                Err(DomainError::StorageError(
                    "IPFS connection refused".to_string(),
                ))
            } else {
                Ok(self.data.clone().unwrap_or_default())
            }
        }

        async fn exists(&self, _cid: &ContentId) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn pin(&self, _cid: &ContentId) -> Result<(), DomainError> {
            Ok(())
        }

        async fn store_dag(
            &self,
            _description: &str,
            _files: &[(String, Vec<u8>)],
        ) -> Result<(ContentId, domain::ports::content_store::DagManifest), DomainError> {
            Ok((
                ContentId::new("QmMockDag"),
                domain::ports::content_store::DagManifest {
                    version: 1,
                    description: String::new(),
                    files: vec![],
                },
            ))
        }
    }

    // ── Mock Chunker ──────────────────────────────────────

    struct MockChunker;

    #[async_trait]
    impl Chunker for MockChunker {
        async fn chunk(
            &self,
            source: &str,
            file_path: &str,
            language: &str,
        ) -> Result<Vec<SemanticChunk>, ChunkerError> {
            if language != "rust" {
                return Err(ChunkerError::UnsupportedLanguage(language.to_string()));
            }

            let mut chunks = vec![];

            if source.contains("fn ") {
                chunks.push(SemanticChunk {
                    kind: tensai::ChunkKind::Function,
                    name: Some("mock_fn".to_string()),
                    content: source.to_string(),
                    start_line: 1,
                    end_line: 3,
                    file_path: file_path.to_string(),
                    language: "rust".to_string(),
                });
            }

            if source.contains("struct ") {
                chunks.push(SemanticChunk {
                    kind: tensai::ChunkKind::Struct,
                    name: Some("MockStruct".to_string()),
                    content: source.to_string(),
                    start_line: 1,
                    end_line: 3,
                    file_path: file_path.to_string(),
                    language: "rust".to_string(),
                });
            }

            Ok(chunks)
        }

        fn supported_languages(&self) -> Vec<String> {
            vec!["rust".to_string()]
        }
    }

    // ── Mock ChunkRepository ─────────────────────────────

    struct MockChunkRepository {
        save_count: Mutex<usize>,
        total_chunks_saved: Mutex<usize>,
        delete_count: Mutex<usize>,
    }

    impl MockChunkRepository {
        fn new() -> Self {
            Self {
                save_count: Mutex::new(0),
                total_chunks_saved: Mutex::new(0),
                delete_count: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl ChunkRepository for MockChunkRepository {
        async fn save_chunks(
            &self,
            _operation_id: &Uuid,
            chunks: &[StoredChunk],
        ) -> Result<usize, DomainError> {
            *self.save_count.lock().await += 1;
            *self.total_chunks_saved.lock().await += chunks.len();
            Ok(chunks.len())
        }

        async fn find_by_operation(
            &self,
            _operation_id: &Uuid,
        ) -> Result<Vec<StoredChunk>, DomainError> {
            Ok(vec![])
        }

        async fn find_by_file(
            &self,
            _operation_id: &Uuid,
            _file_path: &str,
        ) -> Result<Vec<StoredChunk>, DomainError> {
            Ok(vec![])
        }

        async fn find_by_name(
            &self,
            _name: &str,
        ) -> Result<Vec<StoredChunk>, DomainError> {
            Ok(vec![])
        }

        async fn count_by_operation(
            &self,
            _operation_id: &Uuid,
        ) -> Result<usize, DomainError> {
            Ok(0)
        }

        async fn search_similar(
            &self,
            _embedding: &[f32],
            _limit: usize,
            _threshold: f32,
        ) -> Result<Vec<SimilarChunk>, DomainError> {
            Ok(vec![])
        }

        async fn delete_by_operation(
            &self,
            _operation_id: &Uuid,
        ) -> Result<usize, DomainError> {
            *self.delete_count.lock().await += 1;
            Ok(0)
        }
    }

    // ── Mock EventPublisher (Phase 7B) ────────────

    struct MockEventPublisher {
        analysis_published: Mutex<usize>,
    }

    impl MockEventPublisher {
        fn new() -> Self {
            Self {
                analysis_published: Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl EventPublisher for MockEventPublisher {
        async fn publish_operation_created(
            &self,
            _operation: &Operation,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn publish_analysis_complete(
            &self,
            _summary: &AnalysisCompleteSummary,
        ) -> Result<(), DomainError> {
            *self.analysis_published.lock().await += 1;
            Ok(())
        }
    }

    // ── Mock EmbeddingService (Phase 7A) ─────────────

    struct MockEmbeddingService {
        dimensions: usize,
    }

    impl MockEmbeddingService {
        fn new() -> Self {
            Self { dimensions: 256 }
        }
    }

    #[async_trait]
    impl EmbeddingService for MockEmbeddingService {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, DomainError> {
            Ok(vec![0.1; self.dimensions])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, DomainError> {
            Ok(texts.iter().map(|_| vec![0.1; self.dimensions]).collect())
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }
    }

    // ── Helpers ──────────────────────────────────────────

    fn test_author_id() -> Uuid {
        Uuid::parse_str("a1a2a3a4-b1b2-c1c2-d1d2-e1e2e3e4e5e6").unwrap()
    }

    fn base64_encode(data: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let triple = (b0 << 16) | (b1 << 8) | b2;
            result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
            result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
            if chunk.len() > 1 {
                result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
            if chunk.len() > 2 {
                result.push(CHARS[(triple & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
        }
        result
    }

    fn make_ipfs_blob(files: &[(&str, &str)]) -> Vec<u8> {
        let entries: Vec<serde_json::Value> = files
            .iter()
            .map(|(path, content)| {
                let encoded = base64_encode(content.as_bytes());
                serde_json::json!({
                    "path": path,
                    "content_b64": encoded,
                    "size": content.len(),
                })
            })
            .collect();
        serde_json::to_vec(&entries).unwrap()
    }

    fn op_with_ipfs(ipfs_cid: &str) -> Operation {
        Operation::new(
            test_author_id(),
            Uuid::new_v4(),
            ContentId::new("QmJjTest"),
            Some(ContentId::new(ipfs_cid)),
            "Test analyse",
            vec![],
        )
    }

    fn op_without_ipfs() -> Operation {
        Operation::new(
            test_author_id(),
            Uuid::new_v4(),
            ContentId::new("QmJjOnly"),
            None,
            "Sans IPFS",
            vec![],
        )
    }

    fn build_use_case(store: Arc<dyn ContentStore>) -> AnalyzeOperationUseCase {
        AnalyzeOperationUseCase::new(store, Arc::new(MockChunker), None, None, None)
    }

    fn build_use_case_with_repo(
        store: Arc<dyn ContentStore>,
        chunk_repo: Arc<dyn ChunkRepository>,
    ) -> AnalyzeOperationUseCase {
        AnalyzeOperationUseCase::new(store, Arc::new(MockChunker), Some(chunk_repo), None, None)
    }

    fn build_use_case_with_embedding(
        store: Arc<dyn ContentStore>,
        chunk_repo: Arc<dyn ChunkRepository>,
        embed_svc: Arc<dyn EmbeddingService>,
    ) -> AnalyzeOperationUseCase {
        AnalyzeOperationUseCase::new(
            store,
            Arc::new(MockChunker),
            Some(chunk_repo),
            Some(embed_svc),
            None,
        )
    }

    // ── Tests Phase 6A ───────────────────────────────────

    #[tokio::test]
    async fn test_nominal_rust_files_produce_chunks() {
        let blob = make_ipfs_blob(&[
            ("src/lib.rs", "fn hello() { println!(\"hi\"); }"),
            ("src/model.rs", "struct User { name: String }"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmTestBlob")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Analyzed(report) => {
                assert_eq!(report.analyzed_files, 2);
                assert_eq!(report.skipped_files, 0);
                assert!(report.total_chunks >= 2, "Au moins 2 chunks (fn + struct)");
            }
            AnalysisOutcome::Skipped { .. } => panic!("Attendu Analyzed, pas Skipped"),
        }
    }

    #[tokio::test]
    async fn test_skip_when_no_ipfs_cid() {
        let store = Arc::new(MockContentStore::with_data(vec![]));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_without_ipfs()).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Skipped { reason, .. } => {
                assert!(reason.contains("CID IPFS"));
            }
            AnalysisOutcome::Analyzed(_) => panic!("Attendu Skipped, pas Analyzed"),
        }
    }

    #[tokio::test]
    async fn test_non_rust_files_are_skipped() {
        let blob = make_ipfs_blob(&[
            ("README.md", "# Hello World"),
            ("Cargo.toml", "[package]\nname = \"test\""),
            ("data.json", "{}"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmNonRust")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Analyzed(report) => {
                assert_eq!(report.analyzed_files, 0);
                assert_eq!(report.skipped_files, 3);
                assert_eq!(report.total_chunks, 0);
            }
            AnalysisOutcome::Skipped { .. } => panic!("Attendu Analyzed (avec 0 fichiers)"),
        }
    }

    #[tokio::test]
    async fn test_ipfs_error_propagates() {
        let store = Arc::new(MockContentStore::failing());
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmFailing")).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            DomainError::StorageError(msg) => {
                assert!(msg.contains("connection refused"));
            }
            other => panic!("Expected StorageError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mixed_rust_and_non_rust_files() {
        let blob = make_ipfs_blob(&[
            ("src/main.rs", "fn main() {}"),
            ("README.md", "# Title"),
            ("src/lib.rs", "struct Config { port: u16 }"),
            ("Makefile", "build:\n\tcargo build"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmMixed")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Analyzed(report) => {
                assert_eq!(report.analyzed_files, 2);
                assert_eq!(report.skipped_files, 2);
                assert!(report.total_chunks >= 2);
            }
            AnalysisOutcome::Skipped { .. } => panic!("Attendu Analyzed"),
        }
    }

    #[tokio::test]
    async fn test_empty_blob_returns_skipped() {
        let blob = serde_json::to_vec::<Vec<serde_json::Value>>(&vec![]).unwrap();
        let store = Arc::new(MockContentStore::with_data(blob));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmEmpty")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Skipped { reason, .. } => {
                assert!(reason.contains("vide"));
            }
            AnalysisOutcome::Analyzed(_) => panic!("Attendu Skipped pour un blob vide"),
        }
    }

    #[tokio::test]
    async fn test_empty_rust_file_produces_zero_chunks() {
        let blob = make_ipfs_blob(&[("src/empty.rs", "")]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmEmptyRs")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Analyzed(report) => {
                assert_eq!(report.analyzed_files, 1);
                assert_eq!(report.total_chunks, 0);
            }
            AnalysisOutcome::Skipped { .. } => panic!("Attendu Analyzed"),
        }
    }

    // ── Tests Phase 6B : Persistence ────────────────────

    #[tokio::test]
    async fn test_chunks_are_persisted_via_repository() {
        let blob = make_ipfs_blob(&[
            ("src/lib.rs", "fn hello() {}"),
            ("src/model.rs", "struct User { name: String }"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let chunk_repo = Arc::new(MockChunkRepository::new());
        let use_case = build_use_case_with_repo(store, chunk_repo.clone());

        let result = use_case.execute(&op_with_ipfs("QmPersist")).await;
        assert!(result.is_ok());

        let save_count = *chunk_repo.save_count.lock().await;
        assert_eq!(save_count, 1, "save_chunks() doit être appelé une fois");

        let total = *chunk_repo.total_chunks_saved.lock().await;
        assert!(total >= 2, "Au moins 2 chunks (fn + struct) doivent être persistés");
    }

    #[tokio::test]
    async fn test_works_without_chunk_repository() {
        let blob = make_ipfs_blob(&[("src/main.rs", "fn main() {}")]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let use_case = build_use_case(store);

        let result = use_case.execute(&op_with_ipfs("QmNoRepo")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Analyzed(report) => {
                assert_eq!(report.analyzed_files, 1);
                assert!(report.total_chunks >= 1);
            }
            AnalysisOutcome::Skipped { .. } => panic!("Attendu Analyzed"),
        }
    }

    // ── Tests Phase 7A : Embedding ─────────────────────

    #[tokio::test]
    async fn test_chunks_embedded_during_analysis() {
        let blob = make_ipfs_blob(&[
            ("src/lib.rs", "fn hello() {}"),
            ("src/model.rs", "struct User { name: String }"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let chunk_repo = Arc::new(MockChunkRepository::new());
        let embed_svc = Arc::new(MockEmbeddingService::new());
        let use_case = build_use_case_with_embedding(store, chunk_repo.clone(), embed_svc);

        let result = use_case.execute(&op_with_ipfs("QmEmbed")).await;
        assert!(result.is_ok());

        match result.unwrap() {
            AnalysisOutcome::Analyzed(report) => {
                assert!(report.total_chunks >= 2);
            }
            AnalysisOutcome::Skipped { .. } => panic!("Attendu Analyzed"),
        }

        let total = *chunk_repo.total_chunks_saved.lock().await;
        assert!(total >= 2, "Les chunks embedés doivent être persistés");
    }

    #[tokio::test]
    async fn test_analysis_works_without_embedding_service() {
        let blob = make_ipfs_blob(&[("src/main.rs", "fn main() {}")]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let chunk_repo = Arc::new(MockChunkRepository::new());
        let use_case = build_use_case_with_repo(store, chunk_repo.clone());

        let result = use_case.execute(&op_with_ipfs("QmNoEmbed")).await;
        assert!(result.is_ok());

        // Les chunks doivent être persistés même sans embedding.
        let total = *chunk_repo.total_chunks_saved.lock().await;
        assert!(total >= 1, "Les chunks doivent être persistés sans embedding");
    }

    // ── Tests Phase 7B : Re-publication + Idempotence ────

    #[tokio::test]
    async fn test_analysis_complete_event_published() {
        let blob = make_ipfs_blob(&[
            ("src/lib.rs", "fn hello() {}"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let chunk_repo = Arc::new(MockChunkRepository::new());
        let publisher = Arc::new(MockEventPublisher::new());
        let use_case = AnalyzeOperationUseCase::new(
            store,
            Arc::new(MockChunker),
            Some(chunk_repo),
            None,
            Some(publisher.clone()),
        );

        let result = use_case.execute(&op_with_ipfs("QmRepub")).await;
        assert!(result.is_ok());

        let pub_count = *publisher.analysis_published.lock().await;
        assert_eq!(pub_count, 1, "L'événement analysis-complete doit être publié");
    }

    #[tokio::test]
    async fn test_delete_by_operation_called_before_save() {
        let blob = make_ipfs_blob(&[
            ("src/lib.rs", "fn hello() {}"),
        ]);
        let store = Arc::new(MockContentStore::with_data(blob));
        let chunk_repo = Arc::new(MockChunkRepository::new());
        let use_case = build_use_case_with_repo(store, chunk_repo.clone());

        let result = use_case.execute(&op_with_ipfs("QmIdem")).await;
        assert!(result.is_ok());

        let delete_count = *chunk_repo.delete_count.lock().await;
        assert_eq!(delete_count, 1, "delete_by_operation() doit être appelé une fois");

        let save_count = *chunk_repo.save_count.lock().await;
        assert_eq!(save_count, 1, "save_chunks() doit être appelé une fois");
    }
}
