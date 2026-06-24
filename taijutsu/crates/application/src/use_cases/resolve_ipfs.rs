//! Résolution IPFS dual-format — Type Tag pattern (Phase 8.1).
//!
//! Ce module fournit `resolve_ipfs_files()`, une fonction partagée
//! par `get_ipfs_content`, `analyze_operation`, et `review_operation`
//! pour résoudre les fichiers stockés sur IPFS.
//!
//! ## Type Tag Detection
//! 1. Récupère le blob brut du CID racine via `ContentStore::retrieve()`
//! 2. Parse en `serde_json::Value` pour inspecter la structure
//! 3. Si `json["version"]` + `json["files"]` existent → **Merkle DAG** (Phase 8.1)
//!    - Désérialise en `DagManifest`
//!    - Télécharge chaque fichier individuellement via `ContentStore::retrieve()`
//! 4. Sinon → **Legacy Blob** (Phase 5 — array de `{path, content_b64, size}`)
//!    - Désérialise en `Vec<LegacyFileEntry>` et décode le base64
//!
//! ## Pourquoi ici (couche Application) ?
//! La détection du format est une décision métier, pas une responsabilité
//! de l'adaptateur Infrastructure. Le port `ContentStore` reste agnostique
//! du format — il sait juste stocker/récupérer des blobs.

use tracing::info;

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::content_store::{ContentStore, DagManifest};
#[cfg(test)]
use domain::ports::content_store::DagFileLink;

// ── Types publics ────────────────────────────────────────────────────

/// Fichier résolu depuis IPFS, quel que soit le format de stockage.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    /// Chemin du fichier (ex: `"src/main.rs"`).
    pub path: String,
    /// Contenu du fichier en bytes bruts (UTF-8 pour le code source).
    pub content: Vec<u8>,
    /// Taille du fichier en bytes.
    pub size: usize,
    /// CID IPFS individuel du fichier (uniquement pour le format Merkle DAG).
    pub cid: Option<String>,
}

// ── Fonction principale ──────────────────────────────────────────────

/// Résout les fichiers IPFS quel que soit le format (DAG ou Legacy Blob).
///
/// ## Type Tag Detection
/// - `json["version"]` + `json["files"]` → Merkle DAG (Phase 8.1)
/// - Sinon → Legacy Blob (Phase 5)
///
/// ## Arguments
/// - `store` : le ContentStore pour récupérer les blobs
/// - `ipfs_cid` : le CID racine de l'opération
pub async fn resolve_ipfs_files(
    store: &dyn ContentStore,
    ipfs_cid: &ContentId,
) -> Result<Vec<ResolvedFile>, DomainError> {
    // 1. Récupérer le contenu brut du CID racine.
    let blob = store.retrieve(ipfs_cid).await?;

    // 2. Parser en serde_json::Value pour inspecter la structure.
    let json: serde_json::Value = serde_json::from_slice(&blob).map_err(|e| {
        DomainError::Internal(format!(
            "IPFS blob JSON parse failed (CID: {ipfs_cid}): {e}"
        ))
    })?;

    // 3. Type Tag : détecter le format.
    if json.get("version").is_some() && json.get("files").is_some() {
        // ✅ Nouveau format Merkle DAG (Phase 8.1)
        let manifest: DagManifest = serde_json::from_value(json).map_err(|e| {
            DomainError::Internal(format!("DAG manifest deserialization failed: {e}"))
        })?;

        info!(
            ipfs_cid = %ipfs_cid,
            version = manifest.version,
            file_count = manifest.files.len(),
            "📦 Format détecté : Merkle DAG IPLD (v{})",
            manifest.version,
        );

        resolve_dag_files(store, &manifest).await
    } else {
        // 📦 Legacy format — blob JSON monolithique avec base64
        let entries: Vec<LegacyFileEntry> = serde_json::from_value(json).map_err(|e| {
            DomainError::Internal(format!("Legacy blob deserialization failed: {e}"))
        })?;

        info!(
            ipfs_cid = %ipfs_cid,
            file_count = entries.len(),
            "📦 Format détecté : Legacy Blob JSON (base64)"
        );

        decode_legacy_entries(entries)
    }
}

// ── Résolution Merkle DAG ────────────────────────────────────────────

/// Télécharge chaque fichier du Merkle DAG individuellement.
///
/// Chaque `DagFileLink` contient le CID du fichier — on appelle
/// `ContentStore::retrieve()` pour chacun et on retourne les bytes bruts.
async fn resolve_dag_files(
    store: &dyn ContentStore,
    manifest: &DagManifest,
) -> Result<Vec<ResolvedFile>, DomainError> {
    let mut files = Vec::with_capacity(manifest.files.len());

    for link in &manifest.files {
        let file_cid = ContentId::new(&link.cid);
        let content = store.retrieve(&file_cid).await?;

        files.push(ResolvedFile {
            path: link.path.clone(),
            content,
            size: link.size,
            cid: Some(link.cid.clone()),
        });
    }

    Ok(files)
}

// ── Résolution Legacy Blob ───────────────────────────────────────────

/// Entrée de fichier dans le legacy blob JSON (Phase 5 — Option A).
#[derive(Debug, serde::Deserialize)]
struct LegacyFileEntry {
    path: String,
    content_b64: String,
    size: usize,
}

/// Décode les fichiers du legacy blob JSON (base64 → bytes bruts).
fn decode_legacy_entries(entries: Vec<LegacyFileEntry>) -> Result<Vec<ResolvedFile>, DomainError> {
    let mut files = Vec::with_capacity(entries.len());

    for entry in entries {
        let content = base64_decode(&entry.content_b64).map_err(|e| {
            DomainError::Internal(format!(
                "Legacy base64 decode failed for '{}': {e}",
                entry.path
            ))
        })?;

        files.push(ResolvedFile {
            path: entry.path,
            content,
            size: entry.size,
            cid: None, // Legacy format — pas de CID individuel
        });
    }

    Ok(files)
}

// ── Base64 décodeur RFC 4648 (sans dépendance externe) ─────────────

/// Décode une chaîne base64 RFC 4648 en bytes.
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
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("Caractère base64 invalide: {ch}")),
        };

        buf = (buf << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            output.push(((buf >> bits) & 0xFF) as u8);
        }
    }

    Ok(output)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_tag_detects_dag_manifest() {
        let json = serde_json::json!({
            "version": 1,
            "description": "test commit",
            "files": [
                {"path": "src/main.rs", "cid": "QmTest123", "size": 42}
            ]
        });

        assert!(json.get("version").is_some());
        assert!(json.get("files").is_some());

        let manifest: DagManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].path, "src/main.rs");
        assert_eq!(manifest.files[0].cid, "QmTest123");
    }

    #[test]
    fn test_type_tag_detects_legacy_blob() {
        let json = serde_json::json!([
            {"path": "src/main.rs", "content_b64": "Zm4gbWFpbigpIHt9", "size": 12}
        ]);

        // Legacy format is an array — no "version" or "files" keys
        assert!(json.get("version").is_none());
        assert!(json.get("files").is_none());
    }

    #[test]
    fn test_legacy_base64_decode() {
        let entries = vec![LegacyFileEntry {
            path: "test.rs".to_string(),
            content_b64: "Zm4gbWFpbigpIHt9".to_string(), // "fn main() {}"
            size: 12,
        }];

        let files = decode_legacy_entries(entries).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "test.rs");
        assert_eq!(String::from_utf8_lossy(&files[0].content), "fn main() {}");
        assert!(files[0].cid.is_none());
    }

    #[test]
    fn test_dag_manifest_roundtrip() {
        let manifest = DagManifest {
            version: 1,
            description: "test".to_string(),
            files: vec![
                DagFileLink {
                    path: "src/lib.rs".to_string(),
                    cid: "QmABC123".to_string(),
                    size: 100,
                },
                DagFileLink {
                    path: "src/main.rs".to_string(),
                    cid: "QmDEF456".to_string(),
                    size: 200,
                },
            ],
        };

        let json = serde_json::to_vec(&manifest).unwrap();
        let parsed: DagManifest = serde_json::from_slice(&json).unwrap();
        assert_eq!(manifest, parsed);
    }
}
