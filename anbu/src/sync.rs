//! Module sync — Client HTTP pour synchroniser les checkpoints vers le serveur.
//!
//! Envoie les artifacts capturés localement vers le serveur Taijutsu
//! via `POST /api/v1/repos/{owner}/{repo}/checkpoints` en multipart/form-data.
//! Authentification par Personal Access Token (Basic Auth).
//!
//! ## Protocole Multipart (Phase 28C)
//!
//! Le payload multipart est structuré ainsi :
//! 1. **Champ `metadata`** — JSON contenant id, agent, session_id, message, commit_id
//! 2. **Champs `artifact_N`** — Fichiers binaires streamés depuis le disque
//!
//! Le serveur parse les métadonnées en premier, puis itère sur les artifacts
//! pour les stocker sur IPFS (Genjutsu).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use reqwest::blocking::multipart::{Form, Part};
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

/// Métadonnées JSON envoyées dans le champ `metadata` du multipart.
#[derive(Debug, Serialize)]
struct CheckpointMetadata {
    id: String,
    agent: String,
    session_id: String,
    message: Option<String>,
    commit_id: Option<String>,
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
            .timeout(std::time::Duration::from_secs(300)) // 5 min pour gros fichiers
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            server_url: server.url.trim_end_matches('/').to_string(),
            login: server.login.clone(),
            pat: server.pat.clone(),
        })
    }

    /// Synchronise un checkpoint vers le serveur via multipart/form-data.
    ///
    /// ## Flux
    /// 1. Construit les métadonnées JSON (champ `metadata`)
    /// 2. Ajoute chaque artifact comme champ `artifact_N` (streaming fichier)
    /// 3. POST vers `/api/v1/repos/{owner}/{repo}/checkpoints`
    /// 4. Retourne la réponse serveur (CID IPFS, etc.)
    pub fn sync_checkpoint(
        &self,
        owner: &str,
        repo: &str,
        checkpoint: &Checkpoint,
    ) -> Result<SyncResponse> {
        // 1. Métadonnées JSON (premier champ — le serveur le parse en premier)
        let metadata = CheckpointMetadata {
            id: checkpoint.id.to_string(),
            agent: checkpoint.agent.to_string(),
            session_id: checkpoint.session_id.clone(),
            message: checkpoint.message.clone(),
            commit_id: checkpoint.commit_id.clone(),
        };

        let metadata_json = serde_json::to_string(&metadata)
            .context("Failed to serialize checkpoint metadata")?;

        // 2. Construire le formulaire multipart
        let mut form = Form::new()
            .text("metadata", metadata_json);

        for (i, art) in checkpoint.artifacts.iter().enumerate() {
            let content = fs::read(&art.stored_path)
                .with_context(|| format!("Failed to read artifact: {}", art.stored_path.display()))?;

            let part = Part::bytes(content)
                .file_name(art.filename.clone())
                .mime_str("application/octet-stream")
                .context("Failed to set MIME type")?;

            form = form.part(format!("artifact_{i}"), part);
        }

        // 3. POST vers le serveur
        let url = format!(
            "{}/api/v1/repos/{}/{}/checkpoints",
            self.server_url, owner, repo
        );

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.login, Some(&self.pat))
            .multipart(form)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_metadata_serialization() {
        let metadata = CheckpointMetadata {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            agent: "antigravity".to_string(),
            session_id: "test-session-123".to_string(),
            message: Some("Phase 28C test".to_string()),
            commit_id: None,
        };
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("antigravity"));
        assert!(json.contains("550e8400"));
        assert!(json.contains("Phase 28C test"));
    }

    #[test]
    fn test_purge_nonexistent_dir() {
        // Purge d'un répertoire inexistant ne doit pas échouer
        let result = purge_local_checkpoint(std::path::Path::new("/nonexistent/path/anbu/test"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_sync_response_deserialization() {
        let json = r#"{
            "id": "test-id-123",
            "ipfs_cid": "Qmb9rk4jyqdu...",
            "artifact_count": 3,
            "total_size": 12345
        }"#;
        let response: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "test-id-123");
        assert_eq!(response.artifact_count, 3);
        assert_eq!(response.total_size, 12345);
    }
}
