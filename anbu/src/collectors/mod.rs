//! Collecteurs ANBU — Scan des répertoires d'agents IA.
//!
//! Chaque agent IA (Antigravity, Cursor, etc.) a un `Collector` qui sait
//! scanner ses répertoires de données et extraire les artifacts pertinents.

pub mod antigravity;

use std::path::Path;

use anyhow::Result;

use crate::models::{ArtifactKind, SessionInfo};

/// Artifact collecté avant stockage (données brutes).
#[derive(Debug, Clone)]
pub struct CollectedArtifact {
    /// Nom du fichier.
    pub filename: String,
    /// Type détecté.
    pub kind: ArtifactKind,
    /// Chemin source original.
    pub source_path: std::path::PathBuf,
    /// Contenu brut du fichier.
    pub content: Vec<u8>,
}

/// Trait pour les collecteurs d'agents IA.
///
/// Chaque agent IA (Antigravity, Cursor, etc.) implémente ce trait
/// pour scanner ses données et extraire les artifacts.
#[allow(dead_code)]
pub trait Collector {
    /// Nom de l'agent (ex: "antigravity").
    fn name(&self) -> &str;

    /// Détecte les sessions disponibles, triées par date décroissante.
    fn detect_sessions(&self) -> Result<Vec<SessionInfo>>;

    /// Collecte tous les artifacts d'une session donnée.
    fn collect_session(&self, session_id: &str) -> Result<Vec<CollectedArtifact>>;
}

/// Crée un artifact manuel à partir d'un fichier local (pour --attach).
pub fn create_manual_artifact(path: &Path) -> Result<CollectedArtifact> {
    let content = std::fs::read(path)?;
    let filename = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let kind = ArtifactKind::from_filename(&filename);

    Ok(CollectedArtifact {
        filename,
        kind,
        source_path: path.to_path_buf(),
        content,
    })
}
