//! Artifact Store — Stockage filesystem des checkpoints ANBU.
//!
//! Les artifacts sont stockés dans `.shinobi/anbu/checkpoints/{uuid}/`
//! à la racine du repo courant. Ce répertoire est ajouté automatiquement
//! au `.gitignore` (safeguard Vegapunk A).
//!
//! ## Layout
//! ```text
//! .shinobi/anbu/checkpoints/{uuid}/
//! ├── manifest.json           # Métadonnées du checkpoint
//! ├── implementation_plan.md  # Artifact copié
//! ├── task.md
//! └── overview.txt
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::collectors::CollectedArtifact;
use crate::models::{AgentKind, Artifact, Checkpoint};

/// Gère le stockage local des checkpoints ANBU.
pub struct ArtifactStore {
    /// Racine du repo (répertoire courant).
    repo_root: PathBuf,
    /// Répertoire de staging `.shinobi/anbu/checkpoints/`.
    checkpoints_dir: PathBuf,
}

impl ArtifactStore {
    /// Crée un nouveau store dans le répertoire courant.
    pub fn new() -> Result<Self> {
        let repo_root = std::env::current_dir()
            .context("Failed to get current directory")?;
        let checkpoints_dir = repo_root
            .join(".shinobi")
            .join("anbu")
            .join("checkpoints");

        Ok(Self {
            repo_root,
            checkpoints_dir,
        })
    }

    /// Crée un checkpoint à partir des artifacts collectés.
    ///
    /// 1. Crée le répertoire `.shinobi/anbu/checkpoints/{uuid}/`
    /// 2. Copie chaque artifact avec hash SHA-256
    /// 3. Écrit le `manifest.json`
    pub fn create_checkpoint(
        &self,
        agent: AgentKind,
        session_id: &str,
        message: Option<&str>,
        collected: Vec<CollectedArtifact>,
    ) -> Result<Checkpoint> {
        let checkpoint_id = Uuid::new_v4();
        let checkpoint_dir = self.checkpoints_dir.join(checkpoint_id.to_string());

        // Créer le répertoire du checkpoint
        std::fs::create_dir_all(&checkpoint_dir)
            .with_context(|| format!("Failed to create checkpoint dir: {}", checkpoint_dir.display()))?;

        // Stocker chaque artifact
        let mut artifacts = Vec::new();
        for collected_artifact in collected {
            let stored_path = checkpoint_dir.join(&collected_artifact.filename);

            // Écrire le fichier
            std::fs::write(&stored_path, &collected_artifact.content)
                .with_context(|| format!("Failed to write artifact: {}", stored_path.display()))?;

            // Hash SHA-256
            let hash = compute_sha256(&collected_artifact.content);

            artifacts.push(Artifact {
                id: Uuid::new_v4(),
                kind: collected_artifact.kind,
                filename: collected_artifact.filename,
                source_path: collected_artifact.source_path,
                stored_path,
                content_hash: hash,
                size_bytes: collected_artifact.content.len() as u64,
                captured_at: Utc::now(),
            });
        }

        let checkpoint = Checkpoint {
            id: checkpoint_id,
            agent,
            session_id: session_id.to_string(),
            message: message.map(String::from),
            commit_id: None,
            repo_path: self.repo_root.to_string_lossy().to_string(),
            artifacts,
            created_at: Utc::now(),
        };

        // Écrire le manifest
        let manifest_path = checkpoint_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&checkpoint)
            .context("Failed to serialize checkpoint manifest")?;
        std::fs::write(&manifest_path, manifest_json)
            .with_context(|| format!("Failed to write manifest: {}", manifest_path.display()))?;

        Ok(checkpoint)
    }

    /// Auto-injecte `/.shinobi/anbu/` dans le `.gitignore` du repo.
    ///
    /// Safeguard Vegapunk A : empêche le versioning accidentel des
    /// mégaoctets de données IA locales.
    pub fn ensure_gitignore(&self) -> Result<()> {
        let gitignore_path = self.repo_root.join(".gitignore");
        let anbu_pattern = "/.shinobi/anbu/";

        if gitignore_path.exists() {
            let content = std::fs::read_to_string(&gitignore_path)
                .context("Failed to read .gitignore")?;

            // Vérifier si la ligne existe déjà (avec ou sans slash final)
            let already_ignored = content.lines().any(|line| {
                let trimmed = line.trim();
                trimmed == anbu_pattern
                    || trimmed == ".shinobi/anbu/"
                    || trimmed == ".shinobi/anbu"
                    || trimmed == "/.shinobi/anbu"
            });

            if !already_ignored {
                // Ajouter à la fin, avec un commentaire explicatif
                let addition = format!(
                    "\n# ANBU — AI context staging (local only, synced via Phase 28B)\n{}\n",
                    anbu_pattern
                );
                std::fs::write(&gitignore_path, format!("{content}{addition}"))
                    .context("Failed to update .gitignore")?;
            }
        } else {
            // Pas de .gitignore — en créer un minimal
            let content = format!(
                "# ANBU — AI context staging (local only, synced via Phase 28B)\n{}\n",
                anbu_pattern
            );
            std::fs::write(&gitignore_path, content)
                .context("Failed to create .gitignore")?;
        }

        Ok(())
    }
}

/// Calcule le hash SHA-256 d'un contenu en hexadécimal.
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sha256() {
        let hash = compute_sha256(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_compute_sha256_empty() {
        let hash = compute_sha256(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
