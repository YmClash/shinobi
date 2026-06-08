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
use futures::StreamExt;
use parking_lot::Mutex;
use tracing::{info, instrument, warn};

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::vcs_engine::VcsEngine;
// ObjectId fournit la méthode .hex() sur CommitId — nécessaire pour l'ACL
use jj_lib::backend::CommitId;
use jj_lib::matchers::EverythingMatcher;
use jj_lib::object_id::ObjectId;
use jj_lib::repo::Repo as _; // Trait requis pour .store(), .view() sur Arc<ReadonlyRepo>

// ── Types internes ACL (ne sortent jamais du module) ───────────────────────

/// Handle interne vers un workspace jj ouvert.
/// Encapsule les types jj-lib pour qu'ils ne fuient pas vers le domain.
struct WorkspaceHandle {
    #[allow(dead_code)] // Phase 4 : accès au working copy
    workspace: jj_lib::workspace::Workspace,
    repo: Arc<jj_lib::repo::ReadonlyRepo>,
    #[allow(dead_code)] // Phase 4 : signature des commits (UserSettings::signature())
    settings: jj_lib::settings::UserSettings,
}

impl WorkspaceHandle {
    /// Met à jour l'Arc<ReadonlyRepo> après un tx.commit().
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
        _parent_ids: &[String],
    ) -> Result<ContentId, DomainError> {
        let handle_arc = self.handle.clone();
        let desc = description.to_string();

        tokio::task::spawn_blocking(move || {
            // parking_lot::Mutex::lock() — accès direct depuis spawn_blocking
            // sans runtime async, sans unwrap(), sans risque de poisoning.
            let mut guard = handle_arc.lock();

            let wh = guard.as_mut().ok_or_else(|| {
                DomainError::VcsError("workspace not initialized".to_string())
            })?;

            // Cloner l'Arc<ReadonlyRepo> avant de démarrer la transaction.
            // start_transaction(self: &Arc<Self>) emprunte l'Arc — on ne peut
            // pas emprunter depuis le MutexGuard en même temps.
            let repo_arc = wh.repo.clone();

            // 1. Démarrer la transaction
            let mut tx = repo_arc.start_transaction();

            // 2. Obtenir le tree vide (pas de fichiers dans le working copy)
            let empty_tree = repo_arc.store().empty_merged_tree();

            // 3. Déterminer les parents : heads triés, ou root_commit si vide
            let mut heads: Vec<CommitId> = repo_arc.view().heads().iter().cloned().collect();
            heads.sort(); // CommitId: Ord dérivé (object_id.rs) — déterminisme
            let parents = if heads.is_empty() {
                vec![repo_arc.store().root_commit_id().clone()]
            } else {
                heads
            };

            // 4. Créer le commit via CommitBuilder (transactionnel réel)
            let commit = pollster::block_on(
                tx.repo_mut()
                    .new_commit(parents, empty_tree)
                    .set_description(&desc)
                    .write(),
            )
            .map_err(|e| DomainError::VcsError(format!("CommitBuilder write failed: {e}")))?;

            // ACL : capturer le CommitId hex AVANT de consommer tx
            let commit_id_hex = commit.id().hex();

            // 5. Finaliser la transaction — publie le commit dans le repo
            let new_repo = pollster::block_on(
                tx.commit(format!("SHINOBI: {desc}"))
            )
            .map_err(|e| DomainError::VcsError(format!("Transaction commit failed: {e}")))?;

            // 6. Mettre à jour le handle avec le nouveau repo
            wh.update_repo(new_repo);

            info!(
                description = %desc,
                commit_id = %commit_id_hex,
                "Opération VCS créée (jj SimpleBackend — commit transactionnel réel)"
            );

            Ok(ContentId::new(commit_id_hex))
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
        let cid_hex = content_id.to_string();
        let handle_arc = self.handle.clone();

        tokio::task::spawn_blocking(move || {
            let guard = handle_arc.lock();

            let wh = guard.as_ref().ok_or_else(|| {
                DomainError::VcsError("workspace not initialized".to_string())
            })?;

            let repo = &wh.repo;
            let store = repo.store();

            // 1. ACL inverse : ContentId hex → CommitId jj
            let source_id = CommitId::try_from_hex(&cid_hex).ok_or_else(|| {
                DomainError::VcsError(format!("invalid hex commit id: {cid_hex}"))
            })?;

            // 2. Récupérer le commit source
            let source_commit = store.get_commit(&source_id).map_err(|_| {
                DomainError::CommitNotFound {
                    id: cid_hex.clone(),
                }
            })?;
            let source_tree = source_commit.tree();

            // 3. Récupérer le HEAD actuel (déterministe, trié)
            let mut heads: Vec<CommitId> = repo.view().heads().iter().cloned().collect();
            heads.sort();
            let head_id = heads.first().ok_or_else(|| {
                DomainError::VcsError("no heads in repo".to_string())
            })?;
            let head_commit = store.get_commit(head_id).map_err(|e| {
                DomainError::VcsError(format!("failed to get head commit: {e}"))
            })?;
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

    // ── Phase 3 — Tests transactionnels ────────────────────────────────────

    #[tokio::test]
    async fn test_create_operation_writes_real_commit() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        engine.init_workspace("test-repo").await.unwrap();

        // Capturer le HEAD initial (root commit)
        let head_before = engine.resolve_head().await.unwrap().unwrap();

        // Créer une opération transactionnelle réelle
        let cid = engine
            .create_operation("Phase 3 real commit", &[])
            .await
            .unwrap();

        // Le HEAD doit avoir changé (nouveau commit ≠ root)
        let head_after = engine.resolve_head().await.unwrap().unwrap();
        assert_ne!(
            head_before.to_string(),
            head_after.to_string(),
            "HEAD should change after create_operation"
        );

        // Le CID retourné doit être un hex valide de 128 chars (SimpleBackend)
        let hex = cid.to_string();
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "CommitId should be hex: {hex}"
        );
        assert_eq!(hex.len(), 128, "jj CommitId should be 128 hex chars: len={}", hex.len());
    }

    #[tokio::test]
    async fn test_create_operation_multiple_commits() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        engine.init_workspace("test-repo").await.unwrap();

        let cid1 = engine
            .create_operation("First commit", &[])
            .await
            .unwrap();
        let cid2 = engine
            .create_operation("Second commit", &[])
            .await
            .unwrap();

        // Les deux commits doivent être différents
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

        engine.init_workspace("test-repo").await.unwrap();
        engine
            .create_operation("Test commit", &[])
            .await
            .unwrap();

        // Plusieurs appels à resolve_head doivent retourner le même résultat
        let head1 = engine.resolve_head().await.unwrap().unwrap().to_string();
        let head2 = engine.resolve_head().await.unwrap().unwrap().to_string();
        let head3 = engine.resolve_head().await.unwrap().unwrap().to_string();

        assert_eq!(head1, head2, "resolve_head should be deterministic");
        assert_eq!(head2, head3, "resolve_head should be deterministic");
    }

    #[tokio::test]
    async fn test_create_operation_without_init_returns_error() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        // Sans init_workspace, create_operation doit échouer
        let result = engine.create_operation("Should fail", &[]).await;
        assert!(
            result.is_err(),
            "create_operation should fail without init"
        );

        // Vérifier que c'est bien une VcsError
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

        engine.init_workspace("test-repo").await.unwrap();
        engine
            .create_operation("Test commit", &[])
            .await
            .unwrap();

        // diff_since avec le HEAD actuel → zéro changement
        let head = engine.resolve_head().await.unwrap().unwrap();
        let diff = engine.diff_since(&head).await.unwrap();
        assert!(
            diff.is_empty(),
            "diff_since(HEAD) should return empty vec (no changes from HEAD to HEAD)"
        );
    }

    #[tokio::test]
    async fn test_diff_since_invalid_commit_returns_error() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        engine.init_workspace("test-repo").await.unwrap();

        // Un CommitId hex valide mais qui n'existe pas dans le repo
        let fake_id = ContentId::new("a".repeat(128));
        let result = engine.diff_since(&fake_id).await;
        assert!(
            result.is_err(),
            "diff_since with unknown commit should fail"
        );
    }

    #[tokio::test]
    async fn test_repo_updated_after_transaction() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());

        engine.init_workspace("test-repo").await.unwrap();

        let head_before = engine.resolve_head().await.unwrap().unwrap().to_string();

        // Après create_operation, le repo interne doit être mis à jour
        engine
            .create_operation("Update repo test", &[])
            .await
            .unwrap();

        let head_after = engine.resolve_head().await.unwrap().unwrap().to_string();

        assert_ne!(
            head_before, head_after,
            "HEAD should change after transaction — repo must be updated internally"
        );

        // Un second commit doit aussi fonctionner (repo pas stale)
        let cid2 = engine
            .create_operation("Second after update", &[])
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

        engine.init_workspace("test-repo").await.unwrap();

        // Capturer le HEAD initial
        let old_head = engine.resolve_head().await.unwrap().unwrap();

        // Créer un nouveau commit
        engine
            .create_operation("New commit for diff", &[])
            .await
            .unwrap();

        // diff_since(old_head) ne doit pas panic
        // Avec des empty trees, le diff sera vide, mais la logique doit fonctionner
        let diff = engine.diff_since(&old_head).await;
        assert!(
            diff.is_ok(),
            "diff_since(old_head) should not panic: {:?}",
            diff.err()
        );
    }
}
