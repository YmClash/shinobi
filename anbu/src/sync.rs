//! Module sync — Client HTTP pour synchroniser les checkpoints vers le serveur.
//!
//! Envoie les artifacts capturés localement vers le serveur Taijutsu
//! via `POST /api/v1/repos/{owner}/{repo}/checkpoints`.
//! Authentification par Personal Access Token (Basic Auth).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::ServerSection;
use crate::models::Checkpoint;

/// Réponse du serveur après création d'un checkpoint.
#[derive(Debug, Deserialize)]
pub struct SyncResponse {
    pub id: String,
    pub ipfs_cid: String,
    pub artifact_count: i32,
    pub total_size: i64,
}

/// Un artifact encodé pour l'envoi au serveur.
#[derive(Debug, Serialize)]
struct ArtifactPayload {
    filename: String,
    content_b64: String,
}

/// Payload JSON envoyé au serveur.
#[derive(Debug, Serialize)]
struct CheckpointPayload {
    id: String,
    agent: String,
    session_id: String,
    message: Option<String>,
    commit_id: Option<String>,
    artifacts: Vec<ArtifactPayload>,
}

/// Client HTTP pour la synchronisation ANBU → Taijutsu.
pub struct AnbuSyncClient {
    client: reqwest::blocking::Client,
    server_url: String,
    login: String,
    pat: String,
}

impl AnbuSyncClient {
    /// Crée un nouveau client de synchronisation.
    pub fn new(server: &ServerSection) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            server_url: server.url.trim_end_matches('/').to_string(),
            login: server.login.clone(),
            pat: server.pat.clone(),
        })
    }

    /// Synchronise un checkpoint vers le serveur.
    ///
    /// ## Flux
    /// 1. Lit chaque artifact depuis le disque local
    /// 2. Encode en base64
    /// 3. POST vers `/api/v1/repos/{owner}/{repo}/checkpoints`
    /// 4. Retourne la réponse serveur (CID IPFS, etc.)
    pub fn sync_checkpoint(
        &self,
        owner: &str,
        repo: &str,
        checkpoint: &Checkpoint,
    ) -> Result<SyncResponse> {
        // 1. Lire et encoder les artifacts
        let mut artifacts = Vec::new();
        for art in &checkpoint.artifacts {
            let content = fs::read(&art.stored_path)
                .with_context(|| format!("Failed to read artifact: {}", art.stored_path.display()))?;

            // Encoder en base64 (inline, pas de crate externe)
            let b64 = base64_encode(&content);

            artifacts.push(ArtifactPayload {
                filename: art.filename.clone(),
                content_b64: b64,
            });
        }

        // 2. Construire le payload
        let payload = CheckpointPayload {
            id: checkpoint.id.to_string(),
            agent: checkpoint.agent.to_string(),
            session_id: checkpoint.session_id.clone(),
            message: checkpoint.message.clone(),
            commit_id: checkpoint.commit_id.clone(),
            artifacts,
        };

        // 3. POST vers le serveur
        let url = format!(
            "{}/api/v1/repos/{}/{}/checkpoints",
            self.server_url, owner, repo
        );

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.login, Some(&self.pat))
            .json(&payload)
            .send()
            .with_context(|| format!("Failed to send checkpoint to {url}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            bail!("Server returned {status}: {body}");
        }

        let sync_response: SyncResponse = response
            .json()
            .context("Failed to parse server response")?;

        Ok(sync_response)
    }
}

/// Supprime les artifacts locaux après synchronisation réussie.
///
/// ## Alerte Vegapunk : Purge physique
/// Une fois que le serveur a répondu 201 Created (et donc que les fichiers
/// sont en sécurité sur IPFS), on supprime le dossier local pour libérer
/// l'espace disque. Le SQLite garde l'historique (anbu log).
pub fn purge_local_checkpoint(checkpoint_dir: &Path) -> Result<()> {
    if checkpoint_dir.exists() {
        fs::remove_dir_all(checkpoint_dir)
            .with_context(|| format!("Failed to purge: {}", checkpoint_dir.display()))?;
    }
    Ok(())
}

// ── Base64 encoder (inline, pas de crate externe) ─────────────────────

fn base64_encode(data: &[u8]) -> String {
    const ENCODE_TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(ENCODE_TABLE[((triple >> 18) & 0x3F) as usize] as char);
        result.push(ENCODE_TABLE[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(ENCODE_TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ENCODE_TABLE[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode_hello() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
    }

    #[test]
    fn test_base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn test_base64_encode_roundtrip_concept() {
        let original = b"ANBU checkpoint test data 2026";
        let encoded = base64_encode(original);
        assert!(!encoded.is_empty());
        // The encoded string should be ~33% longer
        assert!(encoded.len() >= original.len());
    }
}
