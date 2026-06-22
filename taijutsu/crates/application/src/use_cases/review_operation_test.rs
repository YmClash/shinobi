//! Tests unitaires pour ReviewOperationUseCase.
//!
//! Utilise des mocks manuels (pattern identique aux tests Tensai).

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;
use domain::ports::llm_service::{LlmResponse, LlmService};
use domain::ports::repository::OperationRepository;
use domain::ports::review_repository::{OperationReview, ReviewRepository};
use domain::ports::vcs_engine::VcsEngine;

use super::{ReviewOperationUseCase, ReviewOutcome};

// ── Mocks ─────────────────────────────────────────────────────────────

struct MockOperationRepo {
    operation: Option<Operation>,
}

#[async_trait]
impl OperationRepository for MockOperationRepo {
    async fn save(&self, _op: &Operation) -> Result<(), DomainError> {
        Ok(())
    }
    async fn find_by_id(&self, _id: &Uuid) -> Result<Option<Operation>, DomainError> {
        Ok(self.operation.clone())
    }
    async fn list_recent(&self, _limit: usize) -> Result<Vec<Operation>, DomainError> {
        Ok(vec![])
    }
    async fn find_by_author(&self, _author_id: &Uuid) -> Result<Vec<Operation>, DomainError> {
        Ok(vec![])
    }
}

struct MockVcsEngine {
    changed_files: Vec<String>,
}

#[async_trait]
impl VcsEngine for MockVcsEngine {
    async fn init_workspace(&self, _name: &str) -> Result<(), DomainError> {
        Ok(())
    }
    async fn create_operation(
        &self,
        _desc: &str,
        _parent_ids: &[String],
        _files: &[(String, Vec<u8>)],
    ) -> Result<ContentId, DomainError> {
        Ok(ContentId::new("mock".to_string()))
    }
    async fn resolve_head(&self) -> Result<Option<ContentId>, DomainError> {
        Ok(None)
    }
    async fn diff_since(&self, _cid: &ContentId) -> Result<Vec<String>, DomainError> {
        Ok(self.changed_files.clone())
    }
}

struct MockLlmService {
    response: String,
}

#[async_trait]
impl LlmService for MockLlmService {
    async fn generate(&self, _prompt: &str, _system: &str) -> Result<LlmResponse, DomainError> {
        Ok(LlmResponse {
            content: self.response.clone(),
            model: "mock-model".to_string(),
            duration_ms: 100,
        })
    }
    fn model_name(&self) -> &str {
        "mock-model"
    }
}

struct MockReviewRepo {
    saved: Mutex<Vec<OperationReview>>,
    deleted_count: u64,
}

#[async_trait]
impl ReviewRepository for MockReviewRepo {
    async fn save_review(&self, review: &OperationReview) -> Result<(), DomainError> {
        self.saved.lock().await.push(review.clone());
        Ok(())
    }
    async fn find_by_operation(
        &self,
        _operation_id: &Uuid,
    ) -> Result<Vec<OperationReview>, DomainError> {
        Ok(self.saved.lock().await.clone())
    }
    async fn delete_by_operation(&self, _operation_id: &Uuid) -> Result<u64, DomainError> {
        Ok(self.deleted_count)
    }
    async fn find_recent_scores(&self, _limit: usize) -> Result<Vec<domain::ports::review_repository::ScorePoint>, DomainError> {
        Ok(vec![])
    }
}

struct MockContentStore {
    blob: Option<Vec<u8>>,
}

#[async_trait]
impl ContentStore for MockContentStore {
    async fn store(&self, _data: &[u8]) -> Result<ContentId, DomainError> {
        Ok(ContentId::new("mock-cid".to_string()))
    }
    async fn retrieve(&self, _cid: &ContentId) -> Result<Vec<u8>, DomainError> {
        match &self.blob {
            Some(b) => Ok(b.clone()),
            None => Err(DomainError::StorageError("not found".to_string())),
        }
    }
    async fn exists(&self, _cid: &ContentId) -> Result<bool, DomainError> {
        Ok(self.blob.is_some())
    }
    async fn pin(&self, _cid: &ContentId) -> Result<(), DomainError> {
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

fn make_operation(with_ipfs: bool, with_parent: bool) -> Operation {
    let ipfs_cid = if with_ipfs {
        Some(ContentId::new("QmTestCid".to_string()))
    } else {
        None
    };
    let parent_ids = if with_parent {
        vec![Uuid::new_v4()]
    } else {
        vec![]
    };
    Operation::new(
        Uuid::new_v4(),
        ContentId::new("test-cid".to_string()),
        ipfs_cid,
        "Test commit".to_string(),
        parent_ids,
    )
}

fn make_ipfs_blob() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!([
        {
            "path": "src/main.rs",
            "content_b64": "Zm4gbWFpbigpIHt9", // "fn main() {}" en base64
            "size": 13
        }
    ]))
    .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_review_nominal() {
    let op = make_operation(true, true);
    let op_id = op.id;

    let review_repo = Arc::new(MockReviewRepo {
        saved: Mutex::new(vec![]),
        deleted_count: 0,
    });

    let uc = ReviewOperationUseCase::new(
        Arc::new(MockOperationRepo {
            operation: Some(op),
        }),
        Arc::new(MockVcsEngine {
            changed_files: vec!["src/main.rs".to_string()],
        }),
        Arc::new(MockLlmService {
            response: "Code de bonne qualité.\n\n## ✅ Points forts\nBien structuré.\n\n<score>0.85</score>".to_string(),
        }),
        review_repo.clone(),
        Arc::new(MockContentStore {
            blob: Some(make_ipfs_blob()),
        }),
    );

    let result = uc.execute(op_id).await.unwrap();
    match result {
        ReviewOutcome::Reviewed { operation_id, .. } => {
            assert_eq!(operation_id, op_id);
        }
        ReviewOutcome::Skipped { reason, .. } => {
            panic!("Expected Reviewed, got Skipped: {reason}");
        }
    }

    // Vérifier que le score a été extrait et le contenu nettoyé.
    let saved = review_repo.saved.lock().await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].score, Some(0.85));
    assert!(!saved[0].content.contains("<score>"));
    assert!(!saved[0].content.contains("</score>"));
    assert!(saved[0].content.contains("Bien structuré"));
}

#[tokio::test]
async fn test_review_root_operation_proceeds() {
    let op = make_operation(true, false); // pas de parent = racine, mais avec IPFS
    let op_id = op.id;

    let uc = ReviewOperationUseCase::new(
        Arc::new(MockOperationRepo {
            operation: Some(op),
        }),
        Arc::new(MockVcsEngine {
            changed_files: vec![], // diff vide — normal pour une racine
        }),
        Arc::new(MockLlmService {
            response: "Code initial bien structuré.".to_string(),
        }),
        Arc::new(MockReviewRepo {
            saved: Mutex::new(vec![]),
            deleted_count: 0,
        }),
        Arc::new(MockContentStore {
            blob: Some(make_ipfs_blob()),
        }),
    );

    let result = uc.execute(op_id).await.unwrap();
    // Root operations SHOULD be reviewed (not skipped).
    assert!(matches!(result, ReviewOutcome::Reviewed { .. }));
}

#[tokio::test]
async fn test_review_skip_not_found() {
    let uc = ReviewOperationUseCase::new(
        Arc::new(MockOperationRepo { operation: None }),
        Arc::new(MockVcsEngine {
            changed_files: vec![],
        }),
        Arc::new(MockLlmService {
            response: "nope".to_string(),
        }),
        Arc::new(MockReviewRepo {
            saved: Mutex::new(vec![]),
            deleted_count: 0,
        }),
        Arc::new(MockContentStore { blob: None }),
    );

    let result = uc.execute(Uuid::new_v4()).await.unwrap();
    assert!(matches!(result, ReviewOutcome::Skipped { .. }));
}

#[tokio::test]
async fn test_review_idempotent_deletes_previous() {
    let op = make_operation(true, true);
    let op_id = op.id;

    let review_repo = Arc::new(MockReviewRepo {
        saved: Mutex::new(vec![]),
        deleted_count: 2, // Simule 2 reviews précédentes supprimées
    });

    let uc = ReviewOperationUseCase::new(
        Arc::new(MockOperationRepo {
            operation: Some(op),
        }),
        Arc::new(MockVcsEngine {
            changed_files: vec!["src/lib.rs".to_string()],
        }),
        Arc::new(MockLlmService {
            response: "Review idempotente.".to_string(),
        }),
        review_repo.clone(),
        Arc::new(MockContentStore {
            blob: Some(make_ipfs_blob()),
        }),
    );

    let result = uc.execute(op_id).await.unwrap();
    assert!(matches!(result, ReviewOutcome::Reviewed { .. }));

    // Vérifier qu'une nouvelle review a été sauvegardée.
    let saved = review_repo.saved.lock().await;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].operation_id, op_id);
}

#[tokio::test]
async fn test_extract_summary() {
    let content = "Code bien structuré et lisible.\n\n## ✅ Points forts\n- Architecture hexagonale respectée";
    let summary = super::extract_summary(content);
    assert_eq!(summary, "Code bien structuré et lisible.");
}

#[tokio::test]
async fn test_build_prompt_includes_files() {
    let prompt = super::build_prompt(
        "Test commit",
        &["src/main.rs".to_string()],
        &[],
    );
    assert!(prompt.contains("Test commit"));
    assert!(prompt.contains("src/main.rs"));
    assert!(prompt.contains("code review"));
}

#[tokio::test]
async fn test_extract_score_valid() {
    // Score standard en fin de réponse.
    assert_eq!(super::extract_score("Bonne review\n\n<score>0.85</score>"), Some(0.85));
    // Score avec espaces internes.
    assert_eq!(super::extract_score("Review\n<score> 0.72 </score>"), Some(0.72));
    // Score à la limite haute (clampé à 1.0).
    assert_eq!(super::extract_score("Code parfait\n<score>1.50</score>"), Some(1.0));
    // Score à la limite basse (clampé à 0.0).
    assert_eq!(super::extract_score("Danger\n<score>-0.5</score>"), Some(0.0));
}

#[tokio::test]
async fn test_extract_score_missing() {
    // Pas de balise score.
    assert_eq!(super::extract_score("Review sans score"), None);
    // Balise ouvrante sans fermante.
    assert_eq!(super::extract_score("Review <score>0.5"), None);
    // Contenu non-numérique.
    assert_eq!(super::extract_score("<score>excellent</score>"), None);
}

#[tokio::test]
async fn test_clean_score_tags() {
    let content = "Code bien structuré.\n\n## Points forts\n- OK\n\n<score>0.85</score>";
    let cleaned = super::clean_score_tags(content);
    assert!(!cleaned.contains("<score>"));
    assert!(!cleaned.contains("</score>"));
    assert!(cleaned.contains("Points forts"));
    assert!(cleaned.ends_with("- OK"));

    // Contenu sans score — retourne l'original.
    let no_score = "Review sans score";
    assert_eq!(super::clean_score_tags(no_score), no_score);
}
