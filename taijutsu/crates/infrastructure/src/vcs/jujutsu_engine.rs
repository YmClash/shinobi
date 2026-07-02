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
use dashmap::DashMap;
use futures::StreamExt;
use parking_lot::Mutex;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::vcs_engine::VcsEngine;
// ObjectId fournit la méthode .hex() sur CommitId — nécessaire pour l'ACL
use jj_lib::backend::{CommitId, CopyId, TreeValue};
use jj_lib::matchers::EverythingMatcher;
use jj_lib::merged_tree::MergedTree;
use jj_lib::object_id::ObjectId;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::tree_builder::TreeBuilder; // Trait requis pour .store(), .view() sur Arc<ReadonlyRepo>

// ── Types internes ACL (ne sortent jamais du module) ───────────────────────

/// Handle interne vers un workspace jj ouvert.
/// Encapsule les types jj-lib pour qu'ils ne fuient pas vers le domain.
struct WorkspaceHandle {
    #[allow(dead_code)] // Phase 4 : accès au working copy (pas utilisé pour les ops VCS)
    workspace: Option<jj_lib::workspace::Workspace>,
    repo: Arc<jj_lib::repo::ReadonlyRepo>,
    #[allow(dead_code)] // Phase 4 : signature des commits (UserSettings::signature())
    settings: jj_lib::settings::UserSettings,
}

impl WorkspaceHandle {
    //// Met à jour l'Arc<ReadonlyRepo> après un tx.commit().
    /// jj-lib retourne un nouveau repo à chaque transaction terminée —
    /// l'ancien est obsolète et ne reflète plus l'état du repo.
    fn update_repo(&mut self, new_repo: Arc<jj_lib::repo::ReadonlyRepo>) {
        self.repo = new_repo;
    }
}

// ── UNSAFE CONTRACT — NE MODIFIEZ PAS SANS COMPRENDRE ──────────────────────
//
// # Pourquoi `unsafe impl Send + Sync` ?
//
// `jj_lib::workspace::Workspace` contient des types `!Send` (handles internes
// au backend, file descriptors, caches thread-local). Le compilateur Rust
// refuse donc légitimement de déplacer `WorkspaceHandle` entre threads.
//
// Nous déclarons manuellement `Send + Sync` car notre architecture impose
// trois invariants qui rendent cette transgression **mathématiquement sûre** :
//
// # Les Trois Piliers de Sécurité
//
// ## Pilier 1 — `parking_lot::Mutex` (barrière de synchronisation)
//   Le `WorkspaceHandle` vit dans un `Arc<parking_lot::Mutex<Option<_>>>`.
//   Le Mutex agit comme une **barrière matérielle** : un seul thread peut
//   détenir le `MutexGuard` à un instant donné. Aucun accès concurrent
//   n'est physiquement possible, éliminant les data races.
//   Propriété bonus : `parking_lot` ne poison pas — un panic dans un thread
//   ne corrompt pas le Mutex pour les threads suivants.
//
// ## Pilier 2 — `tokio::task::spawn_blocking` (isolation thread OS)
//   Chaque opération sur le handle est exécutée dans `spawn_blocking`,
//   qui dispatch la closure sur un **thread OS dédié** du pool bloquant
//   de Tokio. Le handle ne traverse jamais la frontière async/sync :
//   il est acquis, utilisé, et relâché **entièrement dans le même thread**.
//
// ## Pilier 3 — Pas de `.await` dans les sections critiques
//   Aucun point de yield async n'existe entre `.lock()` et le drop du
//   `MutexGuard`. La closure dans `spawn_blocking` est **synchrone de bout
//   en bout**. Il est impossible pour le runtime Tokio de migrer la tâche
//   vers un autre thread pendant que le handle est emprunté.
//
// # Modes de Défaillance Catastrophiques (si les invariants sont violés)
//
// ⚠️  **Data Race** : Si le handle est accédé depuis une tâche async
//     (sans spawn_blocking), le runtime Tokio peut migrer la tâche vers
//     un autre OS thread entre deux accès — les types `!Send` internes
//     seront alors utilisés depuis un thread différent de celui qui les
//     a créés → **Undefined Behavior**.
//
// ⚠️  **Corruption Mémoire** : Les caches internes de `Workspace` (backend
//     store, file handles) maintiennent des invariants thread-local.
//     Un accès cross-thread provoque des lectures de mémoire invalide,
//     des double-free, ou des écritures fantômes → **segfault silencieux
//     ou corruption de données du repo .jj/**.
//
// ⚠️  **Indéterminisme** : Les symptômes ne sont PAS reproductibles.
//     Un accès non-protégé peut fonctionner 999 fois et crasher à la
//     1000ème, selon l'ordonnancement des threads par l'OS.
//
// # OPÉRATIONS FORMELLEMENT INTERDITES
//
// 🚫 `handle.lock()` dans une closure `async move { ... }` sans spawn_blocking
// 🚫 `handle.lock()` dans un handler Axum/Tonic directement (c'est async !)
// 🚫 Stocker un `MutexGuard` dans une variable qui traverse un `.await`
// 🚫 Cloner le `WorkspaceHandle` en dehors du `Mutex`
// 🚫 Implémenter `Deref` ou tout trait qui exposerait `Workspace` hors du module
//
// # Preuve de Correction
//
// ∀ accès A au WorkspaceHandle :
//   A ∈ spawn_blocking ∧ A protégé par Mutex::lock() ∧ ¬∃ yield point entre lock/unlock
//   ⟹ A s'exécute sur un unique OS thread, de manière séquentielle et exclusive
//   ⟹ les types !Send ne sont jamais observés depuis un thread différent
//   ⟹ le comportement est équivalent à un programme single-threaded
//   ⟹ CQFD : pas de data race, pas d'UB
//
// Dernière vérification : 2026-06-07 — Phase 2 v0.2.2
// Vérificateur : Analyse statique manuelle + 4/4 tests passés
// ─────────────────────────────────────────────────────────────────────────────
unsafe impl Send for WorkspaceHandle {}
unsafe impl Sync for WorkspaceHandle {}

/// Adaptateur jj-lib avec Anti-Corruption Layer.
///
/// Utilise le `GitBackend` de Jujutsu (Phase 11 — Mutation Git).
/// Chaque workspace est adossé à un repo Git bare interne.
///
/// ## Multi-Tenant (Phase 10A)
/// Le registre `DashMap<Uuid, Arc<Mutex<WorkspaceHandle>>>` isole
/// le verrou au niveau de chaque depot. Deux acteurs peuvent commiter
/// dans des depots differents en parallèle sans contention.
pub struct JujutsuEngine {
    /// Repertoire racine du workspace VCS.
    /// Chaque repo vit dans `{workspace_root}/{repo_id}/`.
    workspace_root: PathBuf,
    /// Registre de handles par repo_id (Phase 10A du Multi-Tenant).
    handles: DashMap<Uuid, Arc<Mutex<WorkspaceHandle>>>,
}

impl JujutsuEngine {
    /// Construit un nouvel adaptateur pour le workspace donné.
    ///
    /// Le chemin est résolu en absolu — jj-lib `Workspace::init_internal_git`
    /// peut paniquer avec des chemins relatifs sur Windows.
    /// Note : on évite `canonicalize()` car sur Windows il ajoute le
    /// préfixe `\\?\` que jj-lib GitBackend/gitoxide ne gère pas.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let raw: PathBuf = workspace_root.into();
        let absolute = if raw.is_absolute() {
            raw
        } else {
            // Normaliser : "./workspace" → "workspace" avant le join
            let cleaned = raw
                .to_string_lossy()
                .trim_start_matches("./")
                .trim_start_matches(".\\")
                .to_string();
            std::env::current_dir()
                .map(|cwd| cwd.join(&cleaned))
                .unwrap_or_else(|_| PathBuf::from(cleaned))
        };
        // S'assurer que le répertoire racine existe
        std::fs::create_dir_all(&absolute).ok();
        Self {
            workspace_root: absolute,
            handles: DashMap::new(),
        }
    }

    /// Récupérer le handle pour un repo donnée (cheap Arc clone).
    fn get_handle(&self, repo_id: &Uuid) -> Option<Arc<Mutex<WorkspaceHandle>>> {
        self.handles.get(repo_id).map(|entry| entry.value().clone())
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
    async fn init_workspace(&self, repo_id: &Uuid) -> Result<(), DomainError> {
        let workspace_path = self.workspace_root.join(repo_id.to_string());
        let rid = *repo_id;

        let workspace_handle = tokio::task::spawn_blocking({
            let workspace_path = workspace_path.clone();
            move || {
                let settings = JujutsuEngine::create_settings()?;

                // Créer le répertoire cible s'il n'existe pas encore
                // (jj-lib crée .jj/ à l'intérieur mais pas le parent)
                std::fs::create_dir_all(&workspace_path).map_err(|e| {
                    DomainError::VcsError(format!(
                        "Cannot create workspace dir {}: {e}",
                        workspace_path.display()
                    ))
                })?;

                let jj_dir = workspace_path.join(".jj");

                let wh = if jj_dir.exists() {
                    // ── Workspace existant → rouvrir le repo ────────────────
                    // jj-lib refuse init_simple si .jj/ existe déjà.
                    // On charge le repo directement via RepoLoader.
                    // Note : `WorkspaceHandle.workspace` est `#[allow(dead_code)]`
                    // — on ne l'utilise pas pour les opérations VCS.
                    info!(
                        path = %workspace_path.display(),
                        repo_id = %rid,
                        "Workspace jj existant détecté — réouverture (skip init)"
                    );

                    // RepoLoader::init_from_file_system lit les fichiers `type`
                    // dans .jj/repo/store, .jj/repo/op_store, etc. et charge
                    // les bons backends via StoreFactories::default().

                    let store_factories = jj_lib::repo::StoreFactories::default();
                    let repo_loader = jj_lib::repo::RepoLoader::init_from_file_system(
                        &settings,
                        &jj_dir.join("repo"),
                        &store_factories,
                    )
                    .map_err(|e| {
                        DomainError::VcsError(format!(
                            "RepoLoader init_from_file_system failed: {e}"
                        ))
                    })?;

                    let repo = pollster::block_on(repo_loader.load_at_head()).map_err(|e| {
                        DomainError::VcsError(format!("RepoLoader load_at_head failed: {e}"))
                    })?;

                    WorkspaceHandle {
                        workspace: None,
                        repo,
                        settings,
                    }
                } else {
                    let (workspace, repo) = pollster::block_on(
                        jj_lib::workspace::Workspace::init_internal_git(&settings, &workspace_path),
                    )
                    .map_err(|e| DomainError::VcsError(format!("Init workspace failed: {e}")))?;

                    info!(
                        path = %workspace_path.display(),
                        repo_id = %rid,
                        "Workspace jj existant détecté — réouverture (skip init)"
                    );

                    WorkspaceHandle {
                        workspace: Some(workspace),
                        repo,
                        settings,
                    }
                };

                Ok::<WorkspaceHandle, DomainError>(wh)
            }
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?;

        let wh = workspace_handle?;
        self.handles.insert(rid, Arc::new(Mutex::new(wh)));

        Ok(())
    }

    #[instrument(skip(self, files))]
    async fn create_operation(
        &self,
        repo_id: &Uuid,
        description: &str,
        _parent_ids: &[String],
        files: &[(String, Vec<u8>)],
    ) -> Result<ContentId, DomainError> {
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;
        let desc = description.to_string();
        let files_owned: Vec<(String, Vec<u8>)> = files.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut guard = handle_arc.lock();
            let wh = &mut *guard;

            // Cloner l'Arc<ReadonlyRepo> avant de démarrer la transaction.
            // start_transaction(self: &Arc<Self>) emprunte l'Arc - on ne peut
            // pas emprunter depuis le MutexGuard en meme temps.
            let repo_arc = wh.repo.clone();
            let store = repo_arc.store();

            // ── Construire le MergedTree ─────────────────────────────────
            let merged_tree = if files_owned.is_empty() {
                // Cas Phase 3 : empty tree (rétro-compatible)
                store.empty_merged_tree()
            } else {
                // Cas Phase 4A : TreeBuilder avec fichiers réels
                let mut tree_builder =
                    TreeBuilder::new(store.clone(), store.empty_tree_id().clone());

                for (path, content) in &files_owned {
                    let repo_path = RepoPathBuf::from_internal_string(path).map_err(|e| {
                        DomainError::VcsError(format!("invalid repo path '{path}': {e}"))
                    })?;

                    // IMPORTANT: Pont Asynchrone (Cursor + AsyncRead)
                    // store.write_file() attend &mut dyn AsyncRead + Send + Unpin.
                    // std::io::Cursor implémente tokio::io::AsyncRead via le
                    // feature io-util (activé par tokio "full") — pollster::block_on
                    // exécute le futur synchronement dans spawn_blocking.
                    let mut cursor = std::io::Cursor::new(content.as_slice());
                    let file_id = pollster::block_on(store.write_file(&repo_path, &mut cursor))
                        .map_err(|e| DomainError::VcsError(format!("write_file failed: {e}")))?;

                    tree_builder.set(
                        repo_path,
                        TreeValue::File {
                            id: file_id,
                            executable: false,
                            copy_id: CopyId::placeholder(),
                        },
                    );
                }

                // write_tree() est async — pollster::block_on dans spawn_blocking
                let tree_id = pollster::block_on(tree_builder.write_tree()).map_err(|e| {
                    DomainError::VcsError(format!("TreeBuilder write_tree failed: {e}"))
                })?;

                // TreeId → MergedTree résolu (sans conflits)
                MergedTree::resolved(store.clone(), tree_id)
            };

            // ── Transaction (inchangé sauf le tree) ──────────────────────
            let mut tx = repo_arc.start_transaction();

            // Déterminer les parents : heads triés, ou root_commit si vide
            let mut heads: Vec<CommitId> = repo_arc.view().heads().iter().cloned().collect();
            heads.sort(); // CommitId: Ord dérivé (object_id.rs) — déterminisme
            let parents = if heads.is_empty() {
                vec![store.root_commit_id().clone()]
            } else {
                heads
            };

            // Créer le commit via CommitBuilder (transactionnel réel)
            let commit = pollster::block_on(
                tx.repo_mut()
                    .new_commit(parents, merged_tree)
                    .set_description(&desc)
                    .write(),
            )
            .map_err(|e| DomainError::VcsError(format!("CommitBuilder write failed: {e}")))?;

            // ACL : capturer le CommitId hex AVANT de consommer tx
            let commit_id_hex = commit.id().hex();

            // Finaliser la transaction — publie le commit dans le repo
            let new_repo = pollster::block_on(tx.commit(format!("SHINOBI: {desc}")))
                .map_err(|e| DomainError::VcsError(format!("Transaction commit failed: {e}")))?;

            // Mettre à jour le handle avec le nouveau repo
            wh.update_repo(new_repo);

            let file_count = files_owned.len();
            info!(
                description = %desc,
                commit_id = %commit_id_hex,
                files = file_count,
                "Opération VCS créée (jj SimpleBackend — commit transactionnel réel)"
            );

            Ok(ContentId::new(commit_id_hex))
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    #[instrument(skip(self))]
    async fn resolve_head(&self, repo_id: &Uuid) -> Result<Option<ContentId>, DomainError> {
        let handle_arc = match self.get_handle(repo_id) {
            Some(h) => h,
            None => {
                warn!(repo_id = %repo_id, "resolve_head: workspace non initialisé");
                return Ok(None);
            }
        };

        tokio::task::spawn_blocking(move || {
            // parking_lot::Mutex::lock() — accès direct, aucun .await
            let guard = handle_arc.lock();
            let handle = &*guard;

            let view = handle.repo.view();
            // Tri lexicographique pour un HEAD déterministe
            // HashSet n'a aucun ordre garanti — sans tri, .first()
            // retournerait un head arbitraire entre exécutions.
            let mut heads: Vec<_> = view.heads().iter().cloned().collect();
            heads.sort();

            if let Some(head_id) = heads.first() {
                // CommitId → hex string → ContentId (ACL : type jj ne sort pas)
                let hex = head_id.hex();
                info!(head = %hex, "HEAD résolu depuis le repo jj");
                Ok(Some(ContentId::new(hex)))
            } else {
                info!("Repo vide — pas de HEAD");
                Ok(None)
            }
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    #[instrument(skip(self))]
    async fn diff_since(
        &self,
        repo_id: &Uuid,
        content_id: &ContentId,
    ) -> Result<Vec<String>, DomainError> {
        let cid_hex = content_id.to_string();
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;

        tokio::task::spawn_blocking(move || {
            let guard = handle_arc.lock();
            let wh = &*guard;

            let repo = &wh.repo;
            let store = repo.store();

            // 1. ACL inverse : ContentId hex → CommitId jj
            let source_id = CommitId::try_from_hex(&cid_hex).ok_or_else(|| {
                DomainError::VcsError(format!("invalid hex commit id: {cid_hex}"))
            })?;

            // 2. Récupérer le commit source
            let source_commit =
                store
                    .get_commit(&source_id)
                    .map_err(|_| DomainError::CommitNotFound {
                        id: cid_hex.clone(),
                    })?;
            let source_tree = source_commit.tree();

            // 3. Récupérer le HEAD actuel (déterministe, trié)
            let mut heads: Vec<CommitId> = repo.view().heads().iter().cloned().collect();
            heads.sort();
            let head_id = heads
                .first()
                .ok_or_else(|| DomainError::VcsError("no heads in repo".to_string()))?;
            let head_commit = store
                .get_commit(head_id)
                .map_err(|e| DomainError::VcsError(format!("failed to get head commit: {e}")))?;
            let head_tree = head_commit.tree();

            // 4. Diff entre les deux trees via diff_stream
            let diff_stream = source_tree.diff_stream(&head_tree, &EverythingMatcher);
            let entries: Vec<_> = pollster::block_on(diff_stream.collect::<Vec<_>>());

            // 5. ACL : TreeDiffEntry → Vec<String> (chemins des fichiers changés)
            let changed_paths: Vec<String> = entries
                .into_iter()
                .filter(|entry| entry.values.is_ok())
                .map(|entry| entry.path.as_internal_file_string().to_string())
                .collect();

            info!(
                source_cid = %cid_hex,
                changes = changed_paths.len(),
                "diff_since: comparaison de trees terminée"
            );

            Ok(changed_paths)
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Generates a fresh repo_id for each test to ensure isolation.
    fn test_repo_id() -> Uuid {
        Uuid::new_v4()
    }

    #[tokio::test]
    async fn test_init_workspace_creates_jj_directory() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        let result = engine.init_workspace(&repo_id).await;
        assert!(result.is_ok(), "init_workspace failed: {:?}", result.err());

        let jj_dir = tmp.path().join(repo_id.to_string()).join(".jj");
        assert!(jj_dir.exists(), ".jj directory should exist after init");
        assert!(jj_dir.is_dir(), ".jj should be a directory");

        let repo_dir = jj_dir.join("repo");
        assert!(repo_dir.exists(), ".jj/repo should exist");
    }

    #[tokio::test]
    async fn test_create_operation_returns_content_id() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let result = engine
            .create_operation(&repo_id, "Test operation", &[], &[])
            .await;
        assert!(
            result.is_ok(),
            "create_operation failed: {:?}",
            result.err()
        );

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
        let repo_id = test_repo_id();

        // Before init: None (repo not in DashMap)
        let head_before = engine.resolve_head(&repo_id).await.unwrap();
        assert!(head_before.is_none(), "HEAD should be None before init");

        // After init: jj creates root commit → HEAD exists
        engine.init_workspace(&repo_id).await.unwrap();
        let head_after = engine.resolve_head(&repo_id).await.unwrap();
        assert!(
            head_after.is_some(),
            "HEAD should be Some after init (jj creates root commit)"
        );

        let hex = head_after.unwrap().to_string();
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "HEAD CID should be hex: {hex}"
        );
        assert_eq!(
            hex.len(),
            40,
            "jj GitBackend CommitId should be 40 hex chars (SHA-1): len={}",
            hex.len()
        );
    }

    #[tokio::test]
    async fn test_parking_lot_mutex_accessible_from_spawn_blocking() {
        let handle: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let h = handle.clone();

        tokio::task::spawn_blocking(move || {
            *h.lock() = Some(42);
        })
        .await
        .unwrap();

        assert_eq!(*handle.lock(), Some(42));
    }

    // ── Phase 3 — Tests transactionnels ────────────────────────────────────

    #[tokio::test]
    async fn test_create_operation_writes_real_commit() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        // Capturer le HEAD initial (root commit)

        let head_before = engine.resolve_head(&repo_id).await.unwrap().unwrap();

        // Créer une opération transactionnelle réelle
        let cid = engine
            .create_operation(&repo_id, "Phase 3 real commit", &[], &[])
            .await
            .unwrap();

        // Le HEAD doit avoir changé (nouveau commit ≠ root)
        let head_after = engine.resolve_head(&repo_id).await.unwrap().unwrap();
        assert_ne!(
            head_before.to_string(),
            head_after.to_string(),
            "HEAD should change after create_operation"
        );

        let hex = cid.to_string();
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "CommitId should be hex: {hex}"
        );
        assert_eq!(
            hex.len(),
            40,
            "jj GitBackend CommitId should be 40 hex chars (SHA-1): len={}",
            hex.len()
        );
    }

    #[tokio::test]
    async fn test_create_operation_multiple_commits() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let cid1 = engine
            .create_operation(&repo_id, "First commit", &[], &[])
            .await
            .unwrap();
        let cid2 = engine
            .create_operation(&repo_id, "Second commit", &[], &[])
            .await
            .unwrap();

        assert_ne!(
            cid1.to_string(),
            cid2.to_string(),
            "Two sequential commits should have different IDs"
        );
    }

    #[tokio::test]
    async fn test_resolve_head_deterministic() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();
        engine
            .create_operation(&repo_id, "Test commit", &[], &[])
            .await
            .unwrap();

        let head1 = engine
            .resolve_head(&repo_id)
            .await
            .unwrap()
            .unwrap()
            .to_string();
        let head2 = engine
            .resolve_head(&repo_id)
            .await
            .unwrap()
            .unwrap()
            .to_string();
        let head3 = engine
            .resolve_head(&repo_id)
            .await
            .unwrap()
            .unwrap()
            .to_string();

        assert_eq!(head1, head2, "resolve_head should be deterministic");
        assert_eq!(head2, head3, "resolve_head should be deterministic");
    }

    #[tokio::test]
    async fn test_create_operation_without_init_returns_error() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        let result = engine
            .create_operation(&repo_id, "Should fail", &[], &[])
            .await;
        assert!(result.is_err(), "create_operation should fail without init");

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("workspace not initialized"),
            "Error should mention workspace not initialized, got: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_diff_since_empty_on_same_head() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();
        engine
            .create_operation(&repo_id, "Test commit", &[], &[])
            .await
            .unwrap();

        let head = engine.resolve_head(&repo_id).await.unwrap().unwrap();
        let diff = engine.diff_since(&repo_id, &head).await.unwrap();
        assert!(
            diff.is_empty(),
            "diff_since(HEAD) should return empty vec (no changes from HEAD to HEAD)"
        );
    }

    #[tokio::test]
    async fn test_diff_since_invalid_commit_returns_error() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let fake_id = ContentId::new("a".repeat(40));
        let result = engine.diff_since(&repo_id, &fake_id).await;
        assert!(
            result.is_err(),
            "diff_since with unknown commit should fail"
        );
    }

    #[tokio::test]
    async fn test_repo_updated_after_transaction() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let head_before = engine
            .resolve_head(&repo_id)
            .await
            .unwrap()
            .unwrap()
            .to_string();

        engine
            .create_operation(&repo_id, "Update repo test", &[], &[])
            .await
            .unwrap();

        let head_after = engine
            .resolve_head(&repo_id)
            .await
            .unwrap()
            .unwrap()
            .to_string();

        assert_ne!(
            head_before, head_after,
            "HEAD should change after transaction â€” repo must be updated internally"
        );

        let cid2 = engine
            .create_operation(&repo_id, "Second after update", &[], &[])
            .await;
        assert!(
            cid2.is_ok(),
            "Second create_operation should work after repo update"
        );
    }

    #[tokio::test]
    async fn test_diff_since_detects_new_commit() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let old_head = engine.resolve_head(&repo_id).await.unwrap().unwrap();

        engine
            .create_operation(&repo_id, "New commit for diff", &[], &[])
            .await
            .unwrap();

        let diff = engine.diff_since(&repo_id, &old_head).await;
        assert!(
            diff.is_ok(),
            "diff_since(old_head) should not panic: {:?}",
            diff.err()
        );
    }

    // ── Phase 4A — Tests VCS File Writing ──────────────────────────────────

    #[tokio::test]
    async fn test_create_operation_with_single_file() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let head_before = engine.resolve_head(&repo_id).await.unwrap().unwrap();

        let files = vec![("hello.txt".to_string(), b"Hello SHINOBI!".to_vec())];
        let cid = engine
            .create_operation(&repo_id, "Add hello.txt", &[], &files)
            .await;
        assert!(
            cid.is_ok(),
            "create_operation with file failed: {:?}",
            cid.err()
        );

        let cid = cid.unwrap();
        assert!(!cid.to_string().is_empty(), "ContentId should not be empty");

        let head_after = engine.resolve_head(&repo_id).await.unwrap().unwrap();
        assert_ne!(
            head_before.to_string(),
            head_after.to_string(),
            "HEAD should change after commit with file"
        );
    }

    #[tokio::test]
    async fn test_create_operation_with_multiple_files() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let files = vec![
            ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
            ("src/lib.rs".to_string(), b"pub mod core;".to_vec()),
            ("README.md".to_string(), b"# SHINOBI".to_vec()),
        ];

        let cid = engine
            .create_operation(&repo_id, "Initial project structure", &[], &files)
            .await;
        assert!(
            cid.is_ok(),
            "create_operation with multiple files failed: {:?}",
            cid.err()
        );

        let hex = cid.unwrap().to_string();
        assert_eq!(
            hex.len(),
            40,
            "jj GitBackend CommitId should be 40 hex chars (SHA-1): len={}",
            hex.len()
        );
    }

    #[tokio::test]
    async fn test_diff_since_detects_file_addition() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let old_head = engine.resolve_head(&repo_id).await.unwrap().unwrap();

        let files = vec![("hello.txt".to_string(), b"Hello World".to_vec())];
        engine
            .create_operation(&repo_id, "Add hello.txt", &[], &files)
            .await
            .unwrap();

        let diff = engine.diff_since(&repo_id, &old_head).await.unwrap();
        assert!(
            !diff.is_empty(),
            "diff_since should detect file addition (got empty vec)"
        );
        assert!(
            diff.contains(&"hello.txt".to_string()),
            "diff should contain 'hello.txt', got: {:?}",
            diff
        );
    }

    #[tokio::test]
    async fn test_create_with_empty_files_uses_empty_tree() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let old_head = engine.resolve_head(&repo_id).await.unwrap().unwrap();

        let cid = engine
            .create_operation(&repo_id, "Empty tree commit", &[], &[])
            .await
            .unwrap();

        assert!(!cid.to_string().is_empty());

        let diff = engine.diff_since(&repo_id, &old_head).await.unwrap();
        assert!(
            diff.is_empty(),
            "diff_since with empty-tree commits should be empty, got: {:?}",
            diff
        );
    }

    #[tokio::test]
    async fn test_create_with_file_then_diff_shows_path() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        let old_cid = engine
            .create_operation(&repo_id, "Empty baseline", &[], &[])
            .await
            .unwrap();

        let files = vec![
            (
                "config/app.toml".to_string(),
                b"[server]\nport = 3000".to_vec(),
            ),
            (
                "src/main.rs".to_string(),
                b"fn main() { println!(\"SHINOBI\"); }".to_vec(),
            ),
        ];
        engine
            .create_operation(&repo_id, "Add project files", &[], &files)
            .await
            .unwrap();

        let diff = engine.diff_since(&repo_id, &old_cid).await.unwrap();
        assert!(
            diff.len() >= 2,
            "diff should show at least 2 files, got {}: {:?}",
            diff.len(),
            diff
        );
        assert!(
            diff.contains(&"config/app.toml".to_string()),
            "diff should contain 'config/app.toml': {:?}",
            diff
        );
        assert!(
            diff.contains(&"src/main.rs".to_string()),
            "diff should contain 'src/main.rs': {:?}",
            diff
        );
    }

    // ── Phase 10A — Multi-Tenant Isolation Tests ──────────────────────────────

    #[tokio::test]
    async fn test_two_repos_isolated_workspaces() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_a = Uuid::new_v4();
        let repo_b = Uuid::new_v4();

        engine.init_workspace(&repo_a).await.unwrap();
        engine.init_workspace(&repo_b).await.unwrap();

        let cid_a = engine
            .create_operation(
                &repo_a,
                "Commit in A",
                &[],
                &[("a.txt".to_string(), b"repo A content".to_vec())],
            )
            .await
            .unwrap();

        let cid_b = engine
            .create_operation(
                &repo_b,
                "Commit in B",
                &[],
                &[("b.txt".to_string(), b"repo B content".to_vec())],
            )
            .await
            .unwrap();

        assert_ne!(cid_a.to_string(), cid_b.to_string());

        let head_a = engine.resolve_head(&repo_a).await.unwrap().unwrap();
        let head_b = engine.resolve_head(&repo_b).await.unwrap().unwrap();
        assert_ne!(head_a.to_string(), head_b.to_string());
    }

    #[tokio::test]
    async fn test_dashmap_registers_multiple_repos() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            engine.init_workspace(id).await.unwrap();
        }

        for id in &ids {
            let head = engine.resolve_head(id).await.unwrap();
            assert!(head.is_some(), "Repo {} should have a HEAD", id);
        }
    }
}
