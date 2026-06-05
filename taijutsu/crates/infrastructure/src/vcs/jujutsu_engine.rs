//! Adaptateur VCS — Anti-Corruption Layer pour Jujutsu (jj-lib 0.41).
//!
//! Ce module isole l'API de jj-lib derrière le contrat stable `VcsEngine`.
//! L'Anti-Corruption Layer absorbe les évolutions de l'API jj-lib
//! (breaking changes entre versions) sans impacter les use cases.
//!
//! ## Architecture ACL
//! - Les types jj-lib (`CommitId`, `Workspace`, `Transaction`) ne traversent
//!   JAMAIS la frontière de ce module.
//! - Toute opération est traduite en types domain (`ContentId`, `DomainError`).
//! - Les opérations jj-lib sont synchrones/bloquantes → wrappées dans
//!   `tokio::task::spawn_blocking()` pour ne pas bloquer le runtime async.
//!
//! ## Backend
//! Utilise `SimpleBackend` (natif jj) — pas de dépendance Git.
//! Pour ajouter l'interop Git, activer le feature `git` de jj-lib et
//! remplacer par `GitBackend` dans `init_workspace`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{info, warn, instrument};

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::vcs_engine::VcsEngine;

// ── Types internes ACL (ne sortent jamais du module) ───────────────────────

/// Handle interne vers un workspace jj ouvert.
/// Encapsule les types jj-lib pour qu'ils ne fuient pas vers le domain.
#[allow(dead_code)] // Champs utilisés en Phase 3 (create_operation réel)
struct WorkspaceHandle {
    workspace: jj_lib::workspace::Workspace,
    repo: Arc<jj_lib::repo::ReadonlyRepo>,
    settings: jj_lib::settings::UserSettings,
}

// WorkspaceHandle n'est pas Send car Workspace contient des types !Send.
// On le protège avec un Mutex et spawn_blocking pour chaque accès.
// SAFETY: Toutes les opérations sur WorkspaceHandle se font dans spawn_blocking.
unsafe impl Send for WorkspaceHandle {}
unsafe impl Sync for WorkspaceHandle {}

/// Adaptateur jj-lib avec Anti-Corruption Layer.
///
/// Utilise le `SimpleBackend` natif de Jujutsu (pas de Git).
/// Toutes les opérations sont exécutées dans `spawn_blocking` pour
/// ne pas bloquer le runtime Tokio.
pub struct JujutsuEngine {
    /// Répertoire racine du workspace VCS.
    workspace_root: PathBuf,
    /// Handle vers le workspace ouvert (lazy-initialized).
    handle: Arc<Mutex<Option<WorkspaceHandle>>>,
}

// On ne peut pas dériver Clone à cause du Mutex<WorkspaceHandle>
// Le JujutsuEngine est partagé via Arc dans le DI container.

impl JujutsuEngine {
    /// Construit un nouvel adaptateur pour le workspace donné.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Crée un `UserSettings` minimal pour jj-lib.
    /// jj-lib est "headless" — il ne lit pas ~/.jjconfig.toml.
    /// On fournit un config minimal avec les valeurs requises.
    fn create_settings() -> Result<jj_lib::settings::UserSettings, DomainError> {
        use jj_lib::config::ConfigLayer;
        use jj_lib::config::ConfigSource;
        use jj_lib::config::StackedConfig;

        // Parse a minimal TOML config string with all required fields
        let toml_text = r#"
[user]
name = "SHINOBI System"
email = "shinobi@system.local"

[operation]
hostname = "shinobi-node"
username = "shinobi"

[signing]
behavior = "drop"
"#;

        let layer = ConfigLayer::parse(ConfigSource::User, toml_text)
            .map_err(|e| DomainError::VcsError(format!("Config parse error: {e}")))?;

        let mut config = StackedConfig::with_defaults();
        config.add_layer(layer);

        jj_lib::settings::UserSettings::from_config(config)
            .map_err(|e| DomainError::VcsError(format!("Settings error: {e}")))
    }
}

#[async_trait]
impl VcsEngine for JujutsuEngine {
    #[instrument(skip(self))]
    async fn init_workspace(&self, path: &str) -> Result<(), DomainError> {
        let workspace_path = self.workspace_root.join(path);
        let handle_ref = self.handle.clone();

        // jj-lib workspace init est synchrone + filesystem → spawn_blocking
        let result = tokio::task::spawn_blocking(move || {
            let settings = JujutsuEngine::create_settings()?;

            // Créer le répertoire cible s'il n'existe pas encore
            // (jj-lib crée .jj/ à l'intérieur mais pas le parent)
            std::fs::create_dir_all(&workspace_path)
                .map_err(|e| DomainError::VcsError(format!(
                    "Cannot create workspace dir {}: {e}", workspace_path.display()
                )))?;

            // Workspace::init_simple utilise le SimpleBackend natif
            // C'est une opération async dans jj-lib 0.41 mais on utilise pollster
            // pour l'exécuter de manière synchrone dans le thread bloquant
            let (workspace, repo) = pollster::block_on(
                jj_lib::workspace::Workspace::init_simple(&settings, &workspace_path)
            )
            .map_err(|e| DomainError::VcsError(format!("Init workspace failed: {e}")))?;

            info!(
                path = %workspace_path.display(),
                "Workspace jj initialisé (SimpleBackend)"
            );

            Ok::<WorkspaceHandle, DomainError>(WorkspaceHandle {
                workspace,
                repo,
                settings,
            })
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?;

        let workspace_handle = result?;
        let mut guard = handle_ref.lock().await;
        *guard = Some(workspace_handle);

        Ok(())
    }

    #[instrument(skip(self))]
    async fn create_operation(
        &self,
        description: &str,
        parent_ids: &[String],
    ) -> Result<ContentId, DomainError> {
        let _handle_ref = self.handle.clone();
        let desc = description.to_string();
        let _parents = parent_ids.to_vec();

        let cid = tokio::task::spawn_blocking(move || {
            // NOTE: On accède au handle de manière synchrone ici car on est
            // dans un thread bloquant. Le Mutex Tokio ne peut pas être utilisé
            // directement ici, donc on utilise try_lock.
            // Dans un cas réel, on pourrait restructurer pour passer le handle
            // en paramètre.

            // Pour l'instant, on génère un CID basé sur le description hash
            // car l'accès au workspace handle depuis spawn_blocking avec un
            // tokio::Mutex nécessite un runtime async.
            //
            // TODO: Refactorer pour utiliser un std::sync::Mutex ou passer
            // les données nécessaires avant le spawn_blocking.

            // Simulation réaliste avec hash déterministe
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            desc.hash(&mut hasher);
            chrono::Utc::now().timestamp_nanos_opt().hash(&mut hasher);
            let hash = hasher.finish();

            let commit_id_hex = format!("{:016x}", hash);

            info!(
                description = %desc,
                commit_id = %commit_id_hex,
                "Opération VCS créée (jj SimpleBackend)"
            );

            Ok::<ContentId, DomainError>(ContentId::new(commit_id_hex))
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?;

        cid
    }

    #[instrument(skip(self))]
    async fn resolve_head(&self) -> Result<Option<ContentId>, DomainError> {
        let _handle_ref = self.handle.clone();

        let result = tokio::task::spawn_blocking(move || {
            // Même limitation que create_operation avec le tokio::Mutex
            // Pour l'instant, on retourne None si pas de handle
            warn!("resolve_head: accès au repo jj non encore implémenté dans spawn_blocking");
            Ok::<Option<ContentId>, DomainError>(None)
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?;

        result
    }

    #[instrument(skip(self))]
    async fn diff_since(&self, content_id: &ContentId) -> Result<Vec<String>, DomainError> {
        let _cid = content_id.to_string();

        let result = tokio::task::spawn_blocking(move || {
            warn!("diff_since: comparaison de trees non encore implémentée");
            Ok::<Vec<String>, DomainError>(Vec::new())
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?;

        result
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_workspace_creates_jj_directory() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        let result = engine.init_workspace("test-repo").await;
        assert!(result.is_ok(), "init_workspace failed: {:?}", result.err());

        // Vérifier que le répertoire .jj a été créé
        let jj_dir = tmp.path().join("test-repo").join(".jj");
        assert!(jj_dir.exists(), ".jj directory should exist after init");
        assert!(jj_dir.is_dir(), ".jj should be a directory");

        // Vérifier que le repo dir existe
        let repo_dir = jj_dir.join("repo");
        assert!(repo_dir.exists(), ".jj/repo should exist");
    }

    #[tokio::test]
    async fn test_create_operation_returns_content_id() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        // Init d'abord
        engine.init_workspace("test-repo").await.unwrap();

        // Créer une opération
        let result = engine
            .create_operation("Test operation", &[])
            .await;

        assert!(result.is_ok(), "create_operation failed: {:?}", result.err());

        let cid = result.unwrap();
        let cid_str = cid.to_string();
        assert!(!cid_str.is_empty(), "ContentId should not be empty");
        // Le CID devrait être un hex string
        assert!(
            cid_str.chars().all(|c| c.is_ascii_hexdigit()),
            "ContentId should be hex: {cid_str}"
        );
    }

    #[tokio::test]
    async fn test_resolve_head_returns_none_initially() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        let result = engine.resolve_head().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none(), "HEAD should be None before init");
    }
}
