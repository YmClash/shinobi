//! Index SQLite ANBU — Recherche rapide des checkpoints et artifacts.
//!
//! La base de données est stockée dans `~/.shinobi/anbu.db` (JAMAIS dans le repo).
//! Utilise le mode WAL (Write-Ahead Logging) pour la résilience aux
//! accès concurrents (safeguard Vegapunk C).

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::models::{AgentKind, Artifact, ArtifactKind, Checkpoint};

/// Index SQLite local pour la recherche des checkpoints.
pub struct AnbuIndex {
    conn: Connection,
}

impl AnbuIndex {
    /// Ouvre (ou crée) la base SQLite avec WAL mode activé.
    pub fn open(db_path: &Path) -> Result<Self> {
        // Créer le répertoire parent si nécessaire
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create DB dir: {}", parent.display()))?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open SQLite: {}", db_path.display()))?;

        // Safeguard Vegapunk C : activer WAL mode pour la concurrence
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        // Créer les tables si elles n'existent pas
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS checkpoints (
                id TEXT PRIMARY KEY,
                agent TEXT NOT NULL,
                session_id TEXT NOT NULL,
                message TEXT,
                commit_id TEXT,
                repo_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                synced_at TEXT,
                server_checkpoint_id TEXT
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                checkpoint_id TEXT NOT NULL REFERENCES checkpoints(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                filename TEXT NOT NULL,
                source_path TEXT NOT NULL,
                stored_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                captured_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_checkpoints_commit ON checkpoints(commit_id);
            CREATE INDEX IF NOT EXISTS idx_checkpoints_session ON checkpoints(session_id);
            CREATE INDEX IF NOT EXISTS idx_checkpoints_repo ON checkpoints(repo_path);
            CREATE INDEX IF NOT EXISTS idx_artifacts_hash ON artifacts(content_hash);
            CREATE INDEX IF NOT EXISTS idx_artifacts_checkpoint ON artifacts(checkpoint_id);",
        )?;

        // Migration: ajouter les colonnes de sync si elles n'existent pas
        // (pour les bases créées avant Phase 28B)
        let has_synced = conn
            .prepare("SELECT synced_at FROM checkpoints LIMIT 0")
            .is_ok();
        if !has_synced {
            conn.execute_batch(
                "ALTER TABLE checkpoints ADD COLUMN synced_at TEXT;
                 ALTER TABLE checkpoints ADD COLUMN server_checkpoint_id TEXT;",
            )?;
        }

        Ok(Self { conn })
    }

    /// Insère un checkpoint et ses artifacts dans l'index.
    pub fn insert_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO checkpoints (id, agent, session_id, message, commit_id, repo_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                checkpoint.id.to_string(),
                checkpoint.agent.to_string(),
                checkpoint.session_id,
                checkpoint.message,
                checkpoint.commit_id,
                checkpoint.repo_path,
                checkpoint.created_at.to_rfc3339(),
            ],
        )?;

        for artifact in &checkpoint.artifacts {
            tx.execute(
                "INSERT OR REPLACE INTO artifacts (id, checkpoint_id, kind, filename, source_path, stored_path, content_hash, size_bytes, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    artifact.id.to_string(),
                    checkpoint.id.to_string(),
                    format!("{:?}", artifact.kind),
                    artifact.filename,
                    artifact.source_path.to_string_lossy().to_string(),
                    artifact.stored_path.to_string_lossy().to_string(),
                    artifact.content_hash,
                    artifact.size_bytes,
                    artifact.captured_at.to_rfc3339(),
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Met à jour le commit_id d'un checkpoint existant.
    ///
    /// Appelé après `jj describe` qui mute le commit et change son hash.
    /// La "Ceinture-Bretelles" : on relit le nouveau hash post-mutation
    /// et on le synchronise dans l'index SQLite.
    pub fn update_commit_id(&self, checkpoint_id: &str, commit_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE checkpoints SET commit_id = ?1 WHERE id = ?2",
            params![commit_id, checkpoint_id],
        )?;
        Ok(())
    }

    /// Liste les checkpoints les plus récents.
    pub fn list_checkpoints(&self, limit: usize) -> Result<Vec<Checkpoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, session_id, message, commit_id, repo_path, created_at
             FROM checkpoints
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let checkpoint_rows = stmt.query_map(params![limit as i64], |row| {
            Ok(CheckpointRow {
                id: row.get(0)?,
                agent: row.get(1)?,
                session_id: row.get(2)?,
                message: row.get(3)?,
                commit_id: row.get(4)?,
                repo_path: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut checkpoints = Vec::new();
        for row in checkpoint_rows {
            let row = row?;
            let artifacts = self.get_artifacts(&row.id)?;

            let agent = row.agent.parse::<AgentKind>().unwrap_or(AgentKind::Antigravity);
            let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            checkpoints.push(Checkpoint {
                id: uuid::Uuid::parse_str(&row.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                agent,
                session_id: row.session_id,
                message: row.message,
                commit_id: row.commit_id,
                repo_path: row.repo_path,
                artifacts,
                created_at,
            });
        }

        Ok(checkpoints)
    }

    /// Trouve un checkpoint par ID (préfixe accepté).
    pub fn find_checkpoint(&self, id_prefix: &str) -> Result<Checkpoint> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, session_id, message, commit_id, repo_path, created_at
             FROM checkpoints
             WHERE id LIKE ?1
             ORDER BY created_at DESC
             LIMIT 1",
        )?;

        let pattern = format!("{id_prefix}%");
        let row = stmt
            .query_row(params![pattern], |row| {
                Ok(CheckpointRow {
                    id: row.get(0)?,
                    agent: row.get(1)?,
                    session_id: row.get(2)?,
                    message: row.get(3)?,
                    commit_id: row.get(4)?,
                    repo_path: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|_| anyhow::anyhow!("Checkpoint not found: {id_prefix}"))?;

        let artifacts = self.get_artifacts(&row.id)?;
        let agent = row.agent.parse::<AgentKind>().unwrap_or(AgentKind::Antigravity);
        let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Ok(Checkpoint {
            id: uuid::Uuid::parse_str(&row.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            agent,
            session_id: row.session_id,
            message: row.message,
            commit_id: row.commit_id,
            repo_path: row.repo_path,
            artifacts,
            created_at,
        })
    }

    /// Récupère les artifacts d'un checkpoint.
    fn get_artifacts(&self, checkpoint_id: &str) -> Result<Vec<Artifact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, filename, source_path, stored_path, content_hash, size_bytes, captured_at
             FROM artifacts
             WHERE checkpoint_id = ?1",
        )?;

        let rows = stmt.query_map(params![checkpoint_id], |row| {
            Ok(ArtifactRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                filename: row.get(2)?,
                source_path: row.get(3)?,
                stored_path: row.get(4)?,
                content_hash: row.get(5)?,
                size_bytes: row.get(6)?,
                captured_at: row.get(7)?,
            })
        })?;

        let mut artifacts = Vec::new();
        for row in rows {
            let row = row?;
            let kind = parse_artifact_kind(&row.kind);
            let captured_at = chrono::DateTime::parse_from_rfc3339(&row.captured_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            artifacts.push(Artifact {
                id: uuid::Uuid::parse_str(&row.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                kind,
                filename: row.filename,
                source_path: std::path::PathBuf::from(row.source_path),
                stored_path: std::path::PathBuf::from(row.stored_path),
                content_hash: row.content_hash,
                size_bytes: row.size_bytes as u64,
                captured_at,
            });
        }

        Ok(artifacts)
    }

    /// Marque un checkpoint comme synchronisé avec le serveur.
    pub fn mark_synced(&self, checkpoint_id: &str, server_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE checkpoints SET synced_at = ?1, server_checkpoint_id = ?2 WHERE id = ?3",
            params![now, server_id, checkpoint_id],
        )?;
        Ok(())
    }

    /// Liste les checkpoints non encore synchronisés.
    pub fn list_unsynced(&self) -> Result<Vec<Checkpoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, session_id, message, commit_id, repo_path, created_at
             FROM checkpoints
             WHERE synced_at IS NULL
             ORDER BY created_at DESC",
        )?;

        let checkpoint_rows = stmt.query_map([], |row| {
            Ok(CheckpointRow {
                id: row.get(0)?,
                agent: row.get(1)?,
                session_id: row.get(2)?,
                message: row.get(3)?,
                commit_id: row.get(4)?,
                repo_path: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut checkpoints = Vec::new();
        for row in checkpoint_rows {
            let row = row?;
            let artifacts = self.get_artifacts(&row.id)?;
            let agent = row.agent.parse::<AgentKind>().unwrap_or(AgentKind::Antigravity);
            let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            checkpoints.push(Checkpoint {
                id: uuid::Uuid::parse_str(&row.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                agent,
                session_id: row.session_id,
                message: row.message,
                commit_id: row.commit_id,
                repo_path: row.repo_path,
                artifacts,
                created_at,
            });
        }

        Ok(checkpoints)
    }

    /// Vérifie si un checkpoint est synchronisé.
    #[allow(dead_code)]
    pub fn is_synced(&self, checkpoint_id: &str) -> Result<bool> {
        let synced: Option<String> = self.conn.query_row(
            "SELECT synced_at FROM checkpoints WHERE id LIKE ?1",
            params![format!("{checkpoint_id}%")],
            |row| row.get(0),
        ).unwrap_or(None);
        Ok(synced.is_some())
    }
}

// ── Internal row types ───────────────────────────────────────────────────

struct CheckpointRow {
    id: String,
    agent: String,
    session_id: String,
    message: Option<String>,
    commit_id: Option<String>,
    repo_path: String,
    created_at: String,
}

struct ArtifactRow {
    id: String,
    kind: String,
    filename: String,
    source_path: String,
    stored_path: String,
    content_hash: String,
    size_bytes: i64,
    captured_at: String,
}

/// Parse le kind d'artifact depuis la chaîne stockée en DB.
fn parse_artifact_kind(s: &str) -> ArtifactKind {
    match s {
        "ImplementationPlan" => ArtifactKind::ImplementationPlan,
        "Task" => ArtifactKind::Task,
        "Walkthrough" => ArtifactKind::Walkthrough,
        "ConversationLog" => ArtifactKind::ConversationLog,
        "Media" => ArtifactKind::Media,
        _ => ArtifactKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use chrono::Utc;
    use uuid::Uuid;

    fn temp_db() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("anbu_test_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test.db")
    }

    #[test]
    fn test_create_and_query_checkpoint() {
        let db_path = temp_db();
        let index = AnbuIndex::open(&db_path).unwrap();

        let checkpoint = Checkpoint {
            id: Uuid::new_v4(),
            agent: AgentKind::Antigravity,
            session_id: "test-session-123".to_string(),
            message: Some("Test checkpoint".to_string()),
            commit_id: Some("abc1234".to_string()),
            repo_path: "/test/repo".to_string(),
            artifacts: vec![Artifact {
                id: Uuid::new_v4(),
                kind: ArtifactKind::ImplementationPlan,
                filename: "implementation_plan.md".to_string(),
                source_path: PathBuf::from("/source/plan.md"),
                stored_path: PathBuf::from("/stored/plan.md"),
                content_hash: "abc123hash".to_string(),
                size_bytes: 1024,
                captured_at: Utc::now(),
            }],
            created_at: Utc::now(),
        };

        index.insert_checkpoint(&checkpoint).unwrap();

        let results = index.list_checkpoints(10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "test-session-123");
        assert_eq!(results[0].artifacts.len(), 1);

        // Cleanup
        std::fs::remove_dir_all(db_path.parent().unwrap()).ok();
    }

    #[test]
    fn test_find_by_prefix() {
        let db_path = temp_db();
        let index = AnbuIndex::open(&db_path).unwrap();

        let id = Uuid::new_v4();
        let checkpoint = Checkpoint {
            id,
            agent: AgentKind::Antigravity,
            session_id: "prefix-test".to_string(),
            message: None,
            commit_id: None,
            repo_path: "/test".to_string(),
            artifacts: vec![],
            created_at: Utc::now(),
        };

        index.insert_checkpoint(&checkpoint).unwrap();

        let prefix = &id.to_string()[..8];
        let found = index.find_checkpoint(prefix).unwrap();
        assert_eq!(found.id, id);

        // Cleanup
        std::fs::remove_dir_all(db_path.parent().unwrap()).ok();
    }

    #[test]
    fn test_wal_mode_enabled() {
        let db_path = temp_db();
        let index = AnbuIndex::open(&db_path).unwrap();

        let mode: String = index
            .conn
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");

        // Cleanup
        std::fs::remove_dir_all(db_path.parent().unwrap()).ok();
    }
}
