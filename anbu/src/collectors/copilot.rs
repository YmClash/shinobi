//! Collecteur Copilot — Scanner les sessions GitHub Copilot VS Code.
//!
//! ## Sources de données
//!
//! VS Code stocke les données Copilot dans `workspaceStorage/{hash}/` :
//! 1. **`GitHub.copilot-chat/transcripts/`** — fichiers JSON par session chat
//! 2. **`chatSessions/`** — historique alternatif (format JSONL)
//!
//! ## Chemins Cross-Platform
//!
//! | OS      | Chemin `workspaceStorage`                                      |
//! |---------|---------------------------------------------------------------|
//! | Windows | `%APPDATA%/Code/User/workspaceStorage/`                       |
//! | macOS   | `~/Library/Application Support/Code/User/workspaceStorage/`   |
//! | Linux   | `~/.config/Code/User/workspaceStorage/`                       |
//!
//! ## Filtrage Workspace
//!
//! Chaque sous-dossier `{hash}/` contient un `workspace.json` avec le champ
//! `"folder"` pointant vers le chemin du projet. On compare ce chemin avec
//! `std::env::current_dir()` pour ne capturer que le workspace courant.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::collectors::{CollectedArtifact, Collector};
use crate::models::{AgentKind, ArtifactKind, SessionInfo};

/// Collecteur pour GitHub Copilot (VS Code).
pub struct CopilotCollector {
    /// Chemin du répertoire `workspaceStorage/`.
    workspace_storage_path: PathBuf,
    /// Chemin du workspace courant (pour le filtrage).
    current_workspace: Option<PathBuf>,
    /// Si true, scanner tous les workspaces (mode `--all-workspaces`).
    all_workspaces: bool,
}

impl CopilotCollector {
    /// Crée un nouveau collecteur Copilot.
    ///
    /// - `all_workspaces` : si true, ne filtre pas par workspace courant
    pub fn new(all_workspaces: bool) -> Self {
        let workspace_storage_path = resolve_vscode_workspace_storage();
        let current_workspace = std::env::current_dir().ok();

        Self {
            workspace_storage_path,
            current_workspace,
            all_workspaces,
        }
    }

    /// Trouve le hash de workspace correspondant au répertoire courant.
    ///
    /// Lit chaque `workspace.json` dans les sous-dossiers de `workspaceStorage/`
    /// et compare le champ `"folder"` avec `current_dir()`.
    fn find_workspace_hashes(&self) -> Vec<PathBuf> {
        if !self.workspace_storage_path.exists() {
            return Vec::new();
        }

        let entries = match std::fs::read_dir(&self.workspace_storage_path) {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };

        let mut matching = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let workspace_json = path.join("workspace.json");
            if !workspace_json.exists() {
                continue;
            }

            // Si --all-workspaces, tout inclure
            if self.all_workspaces {
                matching.push(path);
                continue;
            }

            // Sinon, filtrer par workspace courant
            if let Some(ref current) = self.current_workspace {
                if workspace_matches_dir(&workspace_json, current) {
                    matching.push(path);
                }
            }
        }

        matching
    }

    /// Scanne les transcripts Copilot dans un répertoire workspace.
    fn scan_workspace_transcripts(workspace_dir: &Path) -> Vec<TranscriptEntry> {
        let mut entries = Vec::new();

        // Source 1: GitHub.copilot-chat/transcripts/
        let transcripts_dir = workspace_dir
            .join("GitHub.copilot-chat")
            .join("transcripts");
        if transcripts_dir.exists() {
            Self::scan_transcript_dir(&transcripts_dir, &mut entries);
        }

        // Source 2: chatSessions/ (format alternatif)
        let chat_sessions_dir = workspace_dir.join("chatSessions");
        if chat_sessions_dir.exists() {
            Self::scan_transcript_dir(&chat_sessions_dir, &mut entries);
        }

        entries
    }

    /// Scanne un répertoire de transcripts et ajoute les entrées.
    fn scan_transcript_dir(dir: &Path, entries: &mut Vec<TranscriptEntry>) {
        let read_dir = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|f| f.to_str()) {
                Some(f) => f.to_string(),
                None => continue,
            };

            // Accepter .json et .jsonl
            if !filename.ends_with(".json") && !filename.ends_with(".jsonl") {
                continue;
            }

            let modified = path
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| -> DateTime<Utc> { t.into() })
                .unwrap_or_else(|_| Utc::now());

            let size = path
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);

            // Extraire un résumé de la première requête utilisateur
            let summary = Self::extract_summary(&path);

            entries.push(TranscriptEntry {
                id: filename.trim_end_matches(".json")
                    .trim_end_matches(".jsonl")
                    .to_string(),
                path,
                modified,
                size,
                summary,
            });
        }
    }

    /// Extrait un résumé depuis un fichier transcript.
    ///
    /// Cherche le premier message `user.message` ou `type: "user.message"`
    /// et en extrait le texte.
    fn extract_summary(path: &Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;

        // Essayer le format JSON simple (tableau de messages)
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            // Format: array of messages
            if let Some(arr) = json.as_array() {
                for msg in arr {
                    if msg.get("type").and_then(|t| t.as_str()) == Some("user.message") {
                        if let Some(text) = msg.get("text").and_then(|t| t.as_str()) {
                            return Some(truncate_summary(text));
                        }
                    }
                    // Format alternatif avec "role"
                    if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(text) = msg.get("content").and_then(|t| t.as_str()) {
                            return Some(truncate_summary(text));
                        }
                    }
                }
            }
            // Format: objet avec "messages" array
            if let Some(messages) = json.get("messages").and_then(|m| m.as_array()) {
                for msg in messages {
                    if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(text) = msg.get("content").and_then(|t| t.as_str()) {
                            return Some(truncate_summary(text));
                        }
                    }
                }
            }
        }

        // Essayer le format JSONL (ligne par ligne)
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
                if obj.get("type").and_then(|t| t.as_str()) == Some("user.message") {
                    if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                        return Some(truncate_summary(text));
                    }
                }
            }
        }

        None
    }
}

impl Collector for CopilotCollector {
    fn name(&self) -> &str {
        "copilot"
    }

    fn detect_sessions(&self) -> Result<Vec<SessionInfo>> {
        let workspace_dirs = self.find_workspace_hashes();

        let mut sessions = Vec::new();

        for ws_dir in &workspace_dirs {
            let transcripts = Self::scan_workspace_transcripts(ws_dir);

            for entry in transcripts {
                sessions.push(SessionInfo {
                    id: entry.id,
                    agent: AgentKind::Copilot,
                    summary: entry.summary,
                    last_modified: entry.modified,
                    artifact_count: 1, // Chaque transcript est 1 artifact
                    path: entry.path,
                });
            }
        }

        // Trier par date décroissante
        sessions.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

        Ok(sessions)
    }

    fn collect_session(&self, session_id: &str) -> Result<Vec<CollectedArtifact>> {
        let workspace_dirs = self.find_workspace_hashes();

        for ws_dir in &workspace_dirs {
            let transcripts = Self::scan_workspace_transcripts(ws_dir);

            for entry in transcripts {
                if entry.id == session_id {
                    let content = std::fs::read(&entry.path)
                        .with_context(|| {
                            format!("Failed to read transcript: {}", entry.path.display())
                        })?;

                    let filename = entry.path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| format!("{session_id}.json"));

                    return Ok(vec![CollectedArtifact {
                        filename,
                        kind: ArtifactKind::ConversationLog,
                        source_path: entry.path,
                        content,
                    }]);
                }
            }
        }

        anyhow::bail!(
            "Copilot session not found: {session_id}\n\
             Check `anbu sessions --agent copilot` to list available sessions."
        );
    }
}

// ── Internal Types ───────────────────────────────────────────────────────

/// Entrée interne représentant un fichier transcript détecté.
struct TranscriptEntry {
    id: String,
    path: PathBuf,
    modified: DateTime<Utc>,
    #[allow(dead_code)]
    size: u64,
    summary: Option<String>,
}

// ── Helpers Cross-Platform ───────────────────────────────────────────────

/// Résout le chemin `workspaceStorage` selon l'OS.
///
/// Utilise `dirs::config_dir()` pour Linux/macOS et `dirs::data_dir()`
/// pour Windows (APPDATA), avec fallback sur config_dir.
fn resolve_vscode_workspace_storage() -> PathBuf {
    // Essayer d'abord le chemin standard pour chaque OS
    let candidates = [
        // Windows: %APPDATA%/Code/User/workspaceStorage/
        dirs::data_dir().map(|p| p.join("Code").join("User").join("workspaceStorage")),
        // Linux: ~/.config/Code/User/workspaceStorage/
        dirs::config_dir().map(|p| p.join("Code").join("User").join("workspaceStorage")),
        // macOS alt: ~/Library/Application Support/Code/User/workspaceStorage/
        dirs::config_dir().map(|p| p.join("Code").join("User").join("workspaceStorage")),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // Fallback: retourner le chemin standard (même s'il n'existe pas encore)
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Code")
        .join("User")
        .join("workspaceStorage")
}

/// Vérifie si un `workspace.json` correspond au répertoire courant.
///
/// Le fichier `workspace.json` contient un champ `"folder"` avec un URI
/// de type `file:///path/to/project`. On compare ce chemin (normalisé)
/// avec le `current_dir`.
fn workspace_matches_dir(workspace_json: &Path, current_dir: &Path) -> bool {
    let content = match std::fs::read_to_string(workspace_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(j) => j,
        Err(_) => return false,
    };

    // Champ "folder" : URI de type "file:///c%3A/Users/..." ou "file:///home/..."
    let folder_uri = json
        .get("folder")
        .and_then(|f| f.as_str())
        .unwrap_or("");

    // Décoder l'URI en chemin
    let folder_path = uri_to_path(folder_uri);

    // Comparaison normalisée (canonicalize peut échouer si le chemin n'existe plus)
    let normalized_current = normalize_path(current_dir);
    let normalized_folder = normalize_path(&folder_path);

    normalized_current == normalized_folder
}

/// Convertit un URI `file:///...` en chemin système.
fn uri_to_path(uri: &str) -> PathBuf {
    let stripped = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri);

    // Décoder les pourcentages URL (ex: %3A → :, %20 → espace)
    let decoded = url_decode(stripped);

    PathBuf::from(decoded)
}

/// Décodage URL simplifié (couvre les cas courants).
fn url_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }

    result
}

/// Normalise un chemin pour la comparaison (lowercase sur Windows, séparateurs unifiés).
fn normalize_path(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    // Unifier les séparateurs
    let unified = s.replace('\\', "/");
    // Sur Windows, la comparaison doit être case-insensitive
    if cfg!(windows) {
        unified.to_lowercase()
    } else {
        unified
    }
}

/// Tronque un résumé à 80 caractères max.
fn truncate_summary(text: &str) -> String {
    let clean = text.lines().next().unwrap_or(text).trim();
    if clean.len() > 80 {
        format!("{}...", &clean[..77])
    } else {
        clean.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_decode_simple() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("c%3A/Users"), "c:/Users");
        assert_eq!(url_decode("no_encoding"), "no_encoding");
    }

    #[test]
    fn test_uri_to_path() {
        let path = uri_to_path("file:///c%3A/Users/test/project");
        assert!(path.to_string_lossy().contains("Users"));
        assert!(path.to_string_lossy().contains("project"));
    }

    #[test]
    fn test_normalize_path() {
        let p1 = normalize_path(Path::new("C:\\Users\\test\\project"));
        let p2 = normalize_path(Path::new("C:/Users/test/project"));
        // Sur Windows, les deux doivent être identiques
        if cfg!(windows) {
            assert_eq!(p1, p2);
        }
    }

    #[test]
    fn test_truncate_summary() {
        assert_eq!(truncate_summary("Short"), "Short");
        let long = "A".repeat(100);
        let truncated = truncate_summary(&long);
        assert!(truncated.len() <= 83); // 80 chars + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_resolve_workspace_storage() {
        // Should return a valid path (may not exist on CI)
        let path = resolve_vscode_workspace_storage();
        assert!(path.to_string_lossy().contains("workspaceStorage"));
    }

    #[test]
    fn test_extract_summary_jsonl() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("copilot_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"type": "session.start", "timestamp": "2026-07-31"}}"#).unwrap();
        writeln!(f, r#"{{"type": "user.message", "text": "Create a React component"}}"#).unwrap();
        writeln!(f, r#"{{"type": "assistant.message", "text": "Here is your component"}}"#).unwrap();

        let summary = CopilotCollector::extract_summary(&file);
        assert_eq!(summary, Some("Create a React component".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_extract_summary_json_array() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("copilot_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.json");
        let mut f = std::fs::File::create(&file).unwrap();
        write!(f, r#"[{{"role": "user", "content": "Fix the bug"}}, {{"role": "assistant", "content": "Done"}}]"#).unwrap();

        let summary = CopilotCollector::extract_summary(&file);
        assert_eq!(summary, Some("Fix the bug".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }
}
