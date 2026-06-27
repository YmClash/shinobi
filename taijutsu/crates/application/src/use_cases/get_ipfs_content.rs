//! Use Case: GetIpfsContent — Retrieve and decode IPFS content for an operation.
//!
//! Fetches the content stored in IPFS (Genjutsu) for a given operation
//! and returns the individual files with metadata.
//!
//! ## Phase 8.1 — Dual-Format (Type Tag)
//! Utilise `resolve_ipfs_files()` qui détecte automatiquement le format :
//! - **Merkle DAG** (Phase 8.1) : manifeste JSON + fichiers individuels (raw bytes)
//! - **Legacy Blob** (Phase 5) : blob JSON monolithique (base64)

use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
use serde::Serialize;

use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;
use domain::ports::repository::OperationRepository;

use super::resolve_ipfs::resolve_ipfs_files;

/// A decoded file from IPFS.
#[derive(Debug, Serialize)]
pub struct IpfsFile {
    pub path: String,
    pub size: usize,
    pub content: String,
    pub language: Option<String>,
    /// CID IPFS individuel du fichier (uniquement pour le format Merkle DAG).
    pub cid: Option<String>,
}

/// Result of fetching IPFS content for an operation.
#[derive(Debug, Serialize)]
pub struct IpfsContentResult {
    pub operation_id: Uuid,
    pub ipfs_cid: String,
    pub blob_size: usize,
    pub files: Vec<IpfsFile>,
}

pub struct GetIpfsContentUseCase {
    repo: Arc<dyn OperationRepository>,
    content_store: Option<Arc<dyn ContentStore>>,
}

impl GetIpfsContentUseCase {
    pub fn new(
        repo: Arc<dyn OperationRepository>,
        content_store: Option<Arc<dyn ContentStore>>,
    ) -> Self {
        Self { repo, content_store }
    }

    pub async fn execute(&self, operation_id: Uuid) -> Result<IpfsContentResult, DomainError> {
        let operation = self.repo.find_by_id(&operation_id).await?
            .ok_or(DomainError::NotFound { entity_type: "Operation", id: operation_id })?;

        let ipfs_cid = operation.ipfs_content_id
            .as_ref()
            .ok_or_else(|| DomainError::StorageError(format!("No IPFS CID for operation {operation_id}")))?
            .clone();

        let ipfs_cid_str = ipfs_cid.clone().into_inner();

        let cs = self.content_store.as_ref()
            .ok_or_else(|| DomainError::StorageError("IPFS ContentStore unavailable".to_string()))?;

        // Phase 8.1 — Résolution dual-format via Type Tag
        let resolved = resolve_ipfs_files(cs.as_ref(), &ipfs_cid).await?;
        let blob_size: usize = resolved.iter().map(|f| f.size).sum();

        info!(
            operation_id = %operation_id,
            ipfs_cid = %ipfs_cid,
            file_count = resolved.len(),
            blob_size,
            "📥 IPFS Content resolved"
        );

        let files: Vec<IpfsFile> = resolved.into_iter().filter_map(|f| {
            let content = String::from_utf8(f.content).ok()?;
            let language = detect_language(&f.path);
            Some(IpfsFile {
                path: f.path,
                size: f.size,
                content,
                language,
                cid: f.cid,
            })
        }).collect();

        Ok(IpfsContentResult {
            operation_id,
            ipfs_cid: ipfs_cid_str,
            blob_size,
            files,
        })
    }
}

fn detect_language(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust".into()),
        "ts" => Some("typescript".into()),
        "tsx" => Some("tsx".into()),
        "css" => Some("css".into()),
        "py" => Some("python".into()),
        "js" => Some("javascript".into()),
        "jsx" => Some("jsx".into()),
        "json" => Some("json".into()),
        "toml" => Some("toml".into()),
        "yaml" | "yml" => Some("yaml".into()),
        "md" => Some("markdown".into()),
        "html" => Some("html".into()),
        "sql" => Some("sql".into()),
        _ => None,
    }
}
