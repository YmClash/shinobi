//! Collecteur Antigravity — Scanner le brain/ de Gemini/Antigravity.
//!
//! ## Structure du brain/
//! ```text
//! ~/.gemini/antigravity/brain/{conversation-id}/
//! ├── implementation_plan.md          → ImplementationPlan
//! ├── task.md                         → Task
//! ├── walkthrough.md                  → Walkthrough
//! ├── *.metadata.json                 → Métadonnées (ignoré comme artifact)
//! ├── media__*.png                    → Media
//! ├── .system_generated/
//! │   └── logs/overview.txt           → ConversationLog
//! └── .tempmediaStorage/              → IGNORÉ (transitoire)
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::collectors::{CollectedArtifact, Collector};
use crate::models::{AgentKind, ArtifactKind, SessionInfo};

/// Collecteur pour l'agent Antigravity (Gemini/Google DeepMind).
pub struct AntigravityCollector {
    /// Chemin vers le répertoire brain/.
    brain_path: PathBuf,
}

impl AntigravityCollector {
    pub fn new(brain_path: PathBuf) -> Self {
        Self { brain_path }
    }

    /// Extrait un résumé de la session depuis les métadonnées disponibles.
    fn extract_summary(session_dir: &Path) -> Option<String> {
        // Essayer d'abord implementation_plan.md.metadata.json
        let meta_files = [
            "implementation_plan.md.metadata.json",
            "task.md.metadata.json",
            "walkthrough.md.metadata.json",
        ];

        for meta_file in &meta_files {
            let meta_path = session_dir.join(meta_file);
            if meta_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&meta_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        // Antigravity uses lowercase "summary" in metadata.json
                        if let Some(summary) = json.get("summary")
                            .or_else(|| json.get("Summary"))
                            .and_then(|s| s.as_str())
                        {
                            return Some(summary.to_string());
                        }
                    }
                }
            }
        }

        // Fallback : première ligne du overview.txt
        let overview = session_dir
            .join(".system_generated")
            .join("logs")
            .join("overview.txt");
        if overview.exists() {
            if let Ok(content) = std::fs::read_to_string(&overview) {
                let first_meaningful = content
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| {
                        if line.len() > 100 {
                            format!("{}...", &line[..97])
                        } else {
                            line.to_string()
                        }
                    });
                return first_meaningful;
            }
        }

        None
    }

    /// Compte les artifacts principaux dans une session.
    fn count_artifacts(session_dir: &Path) -> usize {
        let artifact_files = [
            "implementation_plan.md",
            "task.md",
            "walkthrough.md",
        ];
        let mut count = 0;

        for f in &artifact_files {
            if session_dir.join(f).exists() {
                count += 1;
            }
        }

        // overview.txt
        let overview = session_dir
            .join(".system_generated")
            .join("logs")
            .join("overview.txt");
        if overview.exists() {
            count += 1;
        }

        count
    }

    /// Récupère la date de dernière modification d'un répertoire de session.
    fn last_modified(session_dir: &Path) -> DateTime<Utc> {
        // Chercher le fichier le plus récemment modifié
        let candidates = [
            session_dir.join("implementation_plan.md"),
            session_dir.join("task.md"),
            session_dir.join("walkthrough.md"),
            session_dir
                .join(".system_generated")
                .join("logs")
                .join("overview.txt"),
        ];

        let mut latest = None;

        for path in &candidates {
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(modified) = meta.modified() {
                    let dt: DateTime<Utc> = modified.into();
                    latest = Some(match latest {
                        Some(prev) if dt > prev => dt,
                        None => dt,
                        Some(prev) => prev,
                    });
                }
            }
        }

        // Fallback : date du répertoire lui-même
        if latest.is_none() {
            if let Ok(meta) = std::fs::metadata(session_dir) {
                if let Ok(modified) = meta.modified() {
                    return modified.into();
                }
            }
        }

        latest.unwrap_or_else(|| Utc::now())
    }
}

impl Collector for AntigravityCollector {
    fn name(&self) -> &str {
        "antigravity"
    }

    fn detect_sessions(&self) -> Result<Vec<SessionInfo>> {
        if !self.brain_path.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();

        let entries = std::fs::read_dir(&self.brain_path)
            .with_context(|| format!("Failed to read brain dir: {}", self.brain_path.display()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Ignorer les répertoires non-UUID (ex: tempmediaStorage)
            if dir_name.len() < 32 || !dir_name.contains('-') {
                continue;
            }

            let artifact_count = Self::count_artifacts(&path);
            // Ignorer les sessions sans artifacts intéressants
            if artifact_count == 0 {
                continue;
            }

            let summary = Self::extract_summary(&path);
            let last_modified = Self::last_modified(&path);

            sessions.push(SessionInfo {
                id: dir_name,
                agent: AgentKind::Antigravity,
                summary,
                last_modified,
                artifact_count,
                path,
            });
        }

        // Trier par date décroissante (plus récent en premier)
        sessions.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

        Ok(sessions)
    }

    fn collect_session(&self, session_id: &str) -> Result<Vec<CollectedArtifact>> {
        let session_dir = self.brain_path.join(session_id);

        if !session_dir.exists() {
            anyhow::bail!(
                "Session not found: {}\nPath: {}",
                session_id,
                session_dir.display()
            );
        }

        let mut artifacts = Vec::new();

        // 1. Artifacts principaux (markdown)
        let primary_files = [
            "implementation_plan.md",
            "task.md",
            "walkthrough.md",
        ];

        for filename in &primary_files {
            let path = session_dir.join(filename);
            if path.exists() {
                if let Ok(content) = std::fs::read(&path) {
                    let kind = ArtifactKind::from_filename(filename);
                    artifacts.push(CollectedArtifact {
                        filename: filename.to_string(),
                        kind,
                        source_path: path,
                        content,
                    });
                }
            }
        }

        // 2. Conversation log (overview.txt)
        let overview = session_dir
            .join(".system_generated")
            .join("logs")
            .join("overview.txt");
        if overview.exists() {
            if let Ok(content) = std::fs::read(&overview) {
                artifacts.push(CollectedArtifact {
                    filename: "overview.txt".to_string(),
                    kind: ArtifactKind::ConversationLog,
                    source_path: overview,
                    content,
                });
            }
        }

        // 3. Media files (screenshots, recordings — pas .tempmediaStorage)
        for entry in std::fs::read_dir(&session_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let filename = match path.file_name().and_then(|f| f.to_str()) {
                Some(f) => f.to_string(),
                None => continue,
            };

            // Seulement les fichiers média (pas les .metadata.json, .resolved, etc.)
            if filename.starts_with("media__")
                && (filename.ends_with(".png")
                    || filename.ends_with(".jpg")
                    || filename.ends_with(".webp"))
            {
                if let Ok(content) = std::fs::read(&path) {
                    artifacts.push(CollectedArtifact {
                        filename: filename.clone(),
                        kind: ArtifactKind::Media,
                        source_path: path,
                        content,
                    });
                }
            }
        }

        Ok(artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_kind_detection() {
        assert_eq!(
            ArtifactKind::from_filename("implementation_plan.md"),
            ArtifactKind::ImplementationPlan
        );
        assert_eq!(ArtifactKind::from_filename("task.md"), ArtifactKind::Task);
        assert_eq!(
            ArtifactKind::from_filename("walkthrough.md"),
            ArtifactKind::Walkthrough
        );
        assert_eq!(
            ArtifactKind::from_filename("overview.txt"),
            ArtifactKind::ConversationLog
        );
        assert_eq!(
            ArtifactKind::from_filename("media__123.png"),
            ArtifactKind::Media
        );
        assert_eq!(
            ArtifactKind::from_filename("random.txt"),
            ArtifactKind::Other
        );
    }
}
