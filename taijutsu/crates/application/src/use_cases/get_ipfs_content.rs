//! Use Case: GetIpfsContent — Retrieve and decode IPFS blob for an operation.
//!
//! Fetches the blob stored in IPFS (Genjutsu) for a given operation,
//! decodes the JSON, and returns the individual files with metadata.

use std::sync::Arc;
use tracing::info;
use uuid::Uuid;
use serde::Serialize;

use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;
use domain::ports::repository::OperationRepository;

/// A decoded file from the IPFS blob.
#[derive(Debug, Serialize)]
pub struct IpfsFile {
    pub path: String,
    pub size: usize,
    pub content: String,
    pub language: Option<String>,
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

        let blob = cs.retrieve(&ipfs_cid).await?;
        let blob_size = blob.len();

        info!(
            operation_id = %operation_id,
            ipfs_cid = %ipfs_cid,
            blob_size,
            "📥 IPFS Content retrieved"
        );

        // Decode blob JSON → file entries
        let entries: Vec<RawFileEntry> = serde_json::from_slice(&blob).map_err(|e| {
            DomainError::Internal(format!("IPFS blob JSON decode failed: {e}"))
        })?;

        let files: Vec<IpfsFile> = entries.into_iter().filter_map(|entry| {
            let decoded = base64_decode(&entry.content_b64).ok()?;
            let content = String::from_utf8(decoded).ok()?;
            let language = detect_language(&entry.path);
            Some(IpfsFile {
                path: entry.path,
                size: entry.size,
                content,
                language,
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

#[derive(serde::Deserialize)]
struct RawFileEntry {
    path: String,
    content_b64: String,
    size: usize,
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

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for ch in input.chars() {
        let val = match ch {
            'A'..='Z' => (ch as u32) - ('A' as u32),
            'a'..='z' => (ch as u32) - ('a' as u32) + 26,
            '0'..='9' => (ch as u32) - ('0' as u32) + 52,
            '+' => 62, '/' => 63,
            _ => return Err(format!("Invalid base64 char: {ch}")),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 { bits -= 8; output.push(((buf >> bits) & 0xFF) as u8); }
    }
    Ok(output)
}
