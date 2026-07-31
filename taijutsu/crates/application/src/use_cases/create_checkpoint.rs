//! Use case : CreateCheckpoint — Réception et stockage d'un checkpoint ANBU.
//!
//! Reçoit les artifacts en multipart depuis le CLI ANBU, les stocke
//! sur IPFS via `store_dag()`, et indexe les métadonnées en PostgreSQL.

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::anbu_checkpoint::AnbuCheckpoint;
use domain::errors::DomainError;
use domain::ports::anbu_repository::AnbuRepository;
use domain::ports::content_store::ContentStore;

/// Commande pour créer un checkpoint ANBU côté serveur.
#[derive(Debug)]
pub struct CreateCheckpointCommand {
    /// UUID du checkpoint (provient du CLI).
    pub id: Uuid,
    /// UUID du dépôt cible (résolu depuis owner/repo).
    pub repository_id: Uuid,
    /// UUID de l'acteur authentifié (depuis le JWT/PAT).
    pub actor_id: Uuid,
    /// Agent IA source (ex: "antigravity").
    pub agent: String,
    /// Session ID de l'agent.
    pub session_id: String,
    /// Message utilisateur.
    pub message: Option<String>,
    /// Référence jj/git.
    pub commit_id: Option<String>,
    /// Fichiers artifacts : (nom, contenu binaire).
    pub artifacts: Vec<(String, Vec<u8>)>,
}

/// Use case : créer un checkpoint ANBU.
pub struct CreateCheckpointUseCase {
    anbu_repo: Arc<dyn AnbuRepository>,
    content_store: Option<Arc<dyn ContentStore>>,
}

impl CreateCheckpointUseCase {
    pub fn new(
        anbu_repo: Arc<dyn AnbuRepository>,
        content_store: Option<Arc<dyn ContentStore>>,
    ) -> Self {
        Self {
            anbu_repo,
            content_store,
        }
    }

    /// Exécute la création du checkpoint.
    ///
    /// ## Flux
    /// 1. Stocke les artifacts sur IPFS via `store_dag()` (si IPFS disponible)
    /// 2. Insère les métadonnées en PostgreSQL
    /// 3. Retourne le checkpoint créé
    pub async fn execute(&self, cmd: CreateCheckpointCommand) -> Result<AnbuCheckpoint, DomainError> {
        let artifact_count = cmd.artifacts.len() as i32;
        let total_size: i64 = cmd.artifacts.iter().map(|(_, data)| data.len() as i64).sum();

        info!(
            checkpoint_id = %cmd.id,
            agent = %cmd.agent,
            session_id = %cmd.session_id,
            artifact_count,
            total_size,
            "ANBU: Creating checkpoint"
        );

        // 1. Stocker sur IPFS si disponible
        let ipfs_cid = if let Some(ref store) = self.content_store {
            let description = cmd.message.as_deref().unwrap_or("ANBU checkpoint");
            let files: Vec<(String, Vec<u8>)> = cmd.artifacts.clone();
            let file_refs: Vec<(String, Vec<u8>)> = files;

            match store.store_dag(description, &file_refs).await {
                Ok((root_cid, manifest)) => {
                    info!(
                        cid = %root_cid,
                        file_count = manifest.files.len(),
                        "ANBU: Artifacts stored on IPFS (Genjutsu)"
                    );
                    // Pin the root CID to prevent garbage collection
                    if let Err(e) = store.pin(&root_cid).await {
                        warn!("ANBU: Failed to pin CID {root_cid}: {e}");
                    }
                    root_cid.to_string()
                }
                Err(e) => {
                    warn!("ANBU: IPFS storage failed, storing CID placeholder: {e}");
                    format!("local-{}", cmd.id)
                }
            }
        } else {
            warn!("ANBU: No IPFS available, using placeholder CID");
            format!("local-{}", cmd.id)
        };

        // 2. Construire l'entité checkpoint
        let checkpoint = AnbuCheckpoint {
            id: cmd.id,
            repository_id: cmd.repository_id,
            actor_id: cmd.actor_id,
            agent: cmd.agent,
            session_id: cmd.session_id,
            message: cmd.message,
            commit_id: cmd.commit_id,
            ipfs_cid,
            artifact_count,
            total_size,
            created_at: Utc::now(),
        };

        // 3. Persister en PostgreSQL
        self.anbu_repo.save_checkpoint(&checkpoint).await?;

        info!(
            checkpoint_id = %checkpoint.id,
            ipfs_cid = %checkpoint.ipfs_cid,
            "ANBU: Checkpoint created successfully"
        );

        Ok(checkpoint)
    }
}
