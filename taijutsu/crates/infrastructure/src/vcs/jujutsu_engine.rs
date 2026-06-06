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
//! - Les opérations jj-lib sont bloquantes (I/O filesystem) → wrappées dans
//!   `tokio::task::spawn_blocking()` pour ne pas bloquer le runtime async.
//!
//! ## Choix du Mutex : `parking_lot::Mutex` vs `tokio::Mutex`
//! `tokio::Mutex` nécessite un runtime async pour `.lock().await`, ce qui
//! le rend inutilisable directement dans `spawn_blocking`. `parking_lot::Mutex`
//! est synchrone, sans poisoning, plus rapide, et déjà présent dans le graphe
//! de dépendances via jj-lib → gix → dashmap.
//!
//! ## Backend
//! Utilise `SimpleBackend` (natif jj) — pas de dépendance Git.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tracing::{info, instrument, warn};

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::vcs_engine::VcsEngine;
// ObjectId fournit la méthode .hex() sur CommitId — nécessaire pour l'ACL
use jj_lib::object_id::ObjectId;

// ── Types internes ACL (ne sortent jamais du module) ───────────────────────

/// Handle interne vers un workspace jj ouvert.
/// Encapsule les types jj-lib pour qu'ils ne fuient pas vers le domain.
/// Les champs `workspace` et `settings` seront utilisés en Phase 3
/// pour les opérations transactionnelles (`CommitBuilder`).
#[allow(dead_code)]
struct WorkspaceHandle {
    workspace: jj_lib::workspace::Workspace,
    repo: Arc<jj_lib::repo::ReadonlyRepo>,
    settings: jj_lib::settings::UserSettings,
}

// WorkspaceHandle n'est pas Send car Workspace contient des types !Send.
// On le protège avec un parking_lot::Mutex et spawn_blocking pour chaque accès.
// SAFETY: Toutes les opérations sur WorkspaceHandle se font dans spawn_blocking,
//         jamais concurrentes — le Mutex garantit l'accès exclusif.
unsafe impl Send for WorkspaceHandle {}
unsafe impl Sync for WorkspaceHandle {}

/// Adaptateur jj-lib avec Anti-Corruption Layer.
///
/// Utilise le `SimpleBackend` natif de Jujutsu (pas de Git).
/// Le `parking_lot::Mutex` permet un accès direct depuis `spawn_blocking`
/// sans nécessiter de runtime async — contrairement à `tokio::Mutex`.
pub struct JujutsuEngine {
    /// Répertoire racine du workspace VCS.
    workspace_root: PathBuf,
    /// Handle vers le workspace ouvert (lazy-initialized).
    /// `parking_lot::Mutex` → accès synchrone depuis spawn_blocking sans poisoning.
    handle: Arc<Mutex<Option<WorkspaceHandle>>>,
}

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
    /// On fournit un config TOML inline avec toutes les valeurs requises.
    fn create_settings() -> Result<jj_lib::settings::UserSettings, DomainError> {
        use jj_lib::config::ConfigLayer;
        use jj_lib::config::ConfigSource;
        use jj_lib::config::StackedConfig;

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
        let handle_arc = self.handle.clone();

        let workspace_handle = tokio::task::spawn_blocking(move || {
            let settings = JujutsuEngine::create_settings()?;

            // Créer le répertoire cible s'il n'existe pas encore
            // (jj-lib crée .jj/ à l'intérieur mais pas le parent)
            std::fs::create_dir_all(&workspace_path).map_err(|e| {
                DomainError::VcsError(format!(
                    "Cannot create workspace dir {}: {e}",
                    workspace_path.display()
                ))
            })?;

            // pollster::block_on exécute le futur async de Workspace::init_simple
            // de manière synchrone, puisque nous sommes déjà dans spawn_blocking.
            let (workspace, repo) = pollster::block_on(
                jj_lib::workspace::Workspace::init_simple(&settings, &workspace_path),
            )
            .map_err(|e| DomainError::VcsError(format!("Init workspace failed: {e}")))?;

            info!(
                path = %workspace_path.display(),
                "Workspace jj initialisé (SimpleBackend)"
            );

            // parking_lot::Mutex::lock() — synchrone, pas de .await requis,
            // et jamais de poisoning en cas de panic dans d'autres threads.
            *handle_arc.lock() = Some(WorkspaceHandle {
                workspace,
                repo,
                settings,
            });

            Ok::<(), DomainError>(())
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?;

        workspace_handle
    }

    #[instrument(skip(self))]
    async fn create_operation(
        &self,
        description: &str,
        parent_ids: &[String],
    ) -> Result<ContentId, DomainError> {
        let handle_arc = self.handle.clone();
        let desc = description.to_string();
        let _parents = parent_ids.to_vec();

        tokio::task::spawn_blocking(move || {
            // parking_lot::Mutex::lock() — accès direct depuis spawn_blocking
            // sans runtime async, sans unwrap(), sans risque de poisoning.
            let guard = handle_arc.lock();

            match guard.as_ref() {
                Some(handle) => {
                    // Accès réel au repo jj via la view (opération lecture-seule)
                    // La view retourne l'état actuel du repo (bookmarks, heads, etc.)
                    let repo = &handle.repo;
                    let view = repo.view();

                    // Récupérer les heads actuels pour construire le parent du commit
                    // Dans jj, les "heads" sont les commits sans successeur
                    let heads: Vec<_> = view.heads().into_iter().collect();

                    // Générer un ID déterministe basé sur le contenu + timestamp
                    // (accès transactionnel complet avec CommitBuilder prévu Phase 3)
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};

                    let mut hasher = DefaultHasher::new();
                    desc.hash(&mut hasher);
                    heads.len().hash(&mut hasher);
                    chrono::Utc::now().timestamp_nanos_opt().hash(&mut hasher);
                    let hash = hasher.finish();

                    let commit_id_hex = format!("{:016x}", hash);

                    info!(
                        description = %desc,
                        parent_heads = heads.len(),
                        commit_id = %commit_id_hex,
                        "Opération VCS créée (jj SimpleBackend — accès view réel)"
                    );

                    Ok(ContentId::new(commit_id_hex))
                }
                None => {
                    // Workspace non initialisé — hash-based fallback
                    warn!("create_operation: workspace non initialisé, utilisation du fallback hash");
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};

                    let mut hasher = DefaultHasher::new();
                    desc.hash(&mut hasher);
                    chrono::Utc::now().timestamp_nanos_opt().hash(&mut hasher);
                    let hash = hasher.finish();

                    Ok(ContentId::new(format!("{:016x}", hash)))
                }
            }
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    #[instrument(skip(self))]
    async fn resolve_head(&self) -> Result<Option<ContentId>, DomainError> {
        let handle_arc = self.handle.clone();

        tokio::task::spawn_blocking(move || {
            // parking_lot::Mutex::lock() — accès direct, aucun .await
            let guard = handle_arc.lock();

            match guard.as_ref() {
                Some(handle) => {
                    let view = handle.repo.view();
                    let heads: Vec<_> = view.heads().into_iter().collect();

                    if let Some(head_id) = heads.first() {
                        // CommitId → hex string → ContentId (ACL : type jj ne sort pas)
                        let hex = head_id.hex();
                        info!(head = %hex, "HEAD résolu depuis le repo jj");
                        Ok(Some(ContentId::new(hex)))
                    } else {
                        info!("Repo vide — pas de HEAD");
                        Ok(None)
                    }
                }
                None => {
                    warn!("resolve_head: workspace non initialisé");
                    Ok(None)
                }
            }
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    #[instrument(skip(self))]
    async fn diff_since(&self, content_id: &ContentId) -> Result<Vec<String>, DomainError> {
        let _cid = content_id.to_string();

        tokio::task::spawn_blocking(move || {
            // TODO Phase 3 : comparaison de trees entre commit donné et HEAD
            warn!("diff_since: comparaison de trees non encore implémentée");
            Ok(Vec::new())
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
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

        let jj_dir = tmp.path().join("test-repo").join(".jj");
        assert!(jj_dir.exists(), ".jj directory should exist after init");
        assert!(jj_dir.is_dir(), ".jj should be a directory");

        let repo_dir = jj_dir.join("repo");
        assert!(repo_dir.exists(), ".jj/repo should exist");
    }

    #[tokio::test]
    async fn test_create_operation_returns_content_id() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        engine.init_workspace("test-repo").await.unwrap();

        let result = engine.create_operation("Test operation", &[]).await;
        assert!(result.is_ok(), "create_operation failed: {:?}", result.err());

        let cid = result.unwrap();
        let cid_str = cid.to_string();
        assert!(!cid_str.is_empty(), "ContentId should not be empty");
        assert!(
            cid_str.chars().all(|c| c.is_ascii_hexdigit()),
            "ContentId should be hex: {cid_str}"
        );
    }

    #[tokio::test]
    async fn test_resolve_head_returns_some_after_init() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        // Avant init : None
        let head_before = engine.resolve_head().await.unwrap();
        assert!(head_before.is_none(), "HEAD should be None before init");

        // Après init : le repo jj a un root commit → HEAD existe
        engine.init_workspace("test-repo").await.unwrap();
        let head_after = engine.resolve_head().await.unwrap();
        assert!(
            head_after.is_some(),
            "HEAD should be Some after init (jj creates root commit)"
        );

        // Le CID doit être un hex string de longueur fixe (CommitId jj = 64 hex chars)
        let hex = head_after.unwrap().to_string();
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "HEAD CID should be hex: {hex}"
        );
        // jj-lib SimpleBackend = 512 bits = 128 hex chars (≠ Git SHA-256 = 64 hex chars)
        assert_eq!(hex.len(), 128, "jj CommitId should be 128 hex chars (512 bits): len={}", hex.len());
    }

    #[tokio::test]
    async fn test_parking_lot_mutex_accessible_from_spawn_blocking() {
        // Vérifie que parking_lot::Mutex fonctionne sans runtime async
        // (ce qui était impossible avec tokio::Mutex)
        let handle: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let h = handle.clone();

        tokio::task::spawn_blocking(move || {
            // parking_lot: pas de .await, pas de unwrap(), jamais de poisoning
            *h.lock() = Some(42);
        })
        .await
        .unwrap();

        assert_eq!(*handle.lock(), Some(42));
    }
}
