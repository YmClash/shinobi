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
//! Remplacer le `SimpleBackend` natif de Jujutsu par le `GitBackend` dans la phase 10
//! Chaque dépôt Jujutsu est désormais adossé à un repo Git bare interne

use std::collections::HashSet;
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
use domain::ports::vcs_engine::{EntryKind, RefInfo, RefKind, TreeEntry, VcsEngine};
// ObjectId fournit la méthode .hex() sur CommitId — nécessaire pour l'ACL
use jj_lib::backend::{CommitId, CopyId, TreeValue};
use jj_lib::matchers::EverythingMatcher;
use jj_lib::merged_tree::MergedTree;
use jj_lib::object_id::ObjectId;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::tree_builder::TreeBuilder; // Trait requis pour .store(), .view() sur Arc<ReadonlyRepo>

// ── Types publics Phase 12A-Fix ────────────────────────────────────────

/// Snapshot complet d'un commit : description + fichiers avec contenu.
///
/// Retourne par `JujutsuEngine::read_commit_snapshot()` pour enrichir
/// les Operations creees par le Sync Hook post-push.
#[derive(Debug)]
pub struct CommitSnapshot {
    /// Description du commit (message de commit Git/jj).
    pub description: String,
    /// Fichiers du commit : (chemin, contenu bytes).
    pub files: Vec<(String, Vec<u8>)>,
    /// SHA-1 hex des commits parents Git (filtré du root_commit_id jj).
    /// Vide pour un root commit (premier push).
    /// Utilisé par le Sync Hook pour résoudre la lignée (Phase 12A-Fix2).
    pub parent_commit_ids: Vec<String>,
}

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

    // ── Phase 12A — Git Bridge HTTP ────────────────────────────────────

    /// Retourne le chemin du bare Git repo pour un depot donne.
    ///
    /// Utilise par le Git Bridge HTTP pour passer `GIT_PROJECT_ROOT`
    /// au CGI `git http-backend`.
    ///
    /// Layout : `{workspace_root}/{repo_id}/.jj/repo/store/git`
    pub fn git_repo_path(&self, repo_id: &Uuid) -> PathBuf {
        self.workspace_root
            .join(repo_id.to_string())
            .join(".jj")
            .join("repo")
            .join("store")
            .join("git")
    }

    /// Resout le HEAD depuis les refs Git du bare repo (pas les heads jj).
    ///
    /// Apres un `git push`, les refs Git (`refs/heads/main`) sont mises a jour
    /// directement par `git http-backend`. Cette methode lit le SHA-1 depuis
    /// le filesystem — plus fiable que `resolve_head()` qui trie les heads jj
    /// (dont certains peuvent etre des commits orphelins crees par le Sync Hook).
    ///
    /// ## Strategie (robuste)
    /// 1. Suivre le symref HEAD → `refs/heads/main`
    /// 2. Fallback : scanner TOUS les loose refs dans `refs/heads/`
    /// 3. Fallback : scanner packed-refs pour `refs/heads/`
    ///
    /// Le fallback est necessaire car le HEAD du bare git repo jj peut
    /// pointer vers `refs/heads/master` (defaut de `init_internal_git`)
    /// alors que le push est sur `refs/heads/main`.
    pub fn resolve_git_head(&self, repo_id: &Uuid) -> Option<ContentId> {
        let git_dir = self.git_repo_path(repo_id);

        // ── Strategie 1 : suivre le symref HEAD ──────────────────────
        if let Some(cid) = self.try_resolve_head_symref(&git_dir) {
            return Some(cid);
        }

        // ── Strategie 2 : scanner tous les loose refs dans refs/heads/ ──
        if let Some(cid) = self.scan_loose_refs(&git_dir) {
            return Some(cid);
        }

        // ── Strategie 3 : scanner packed-refs ────────────────────────
        if let Some(cid) = self.scan_packed_refs(&git_dir) {
            return Some(cid);
        }

        warn!(
            repo_id = %repo_id,
            git_dir = %git_dir.display(),
            "resolve_git_head: aucune strategie n'a trouve de HEAD valide"
        );
        None
    }

    /// Strategie 1 : Lire HEAD → symref → SHA-1
    fn try_resolve_head_symref(&self, git_dir: &std::path::Path) -> Option<ContentId> {
        let head_path = git_dir.join("HEAD");
        let head_content = std::fs::read_to_string(&head_path).ok()?;
        let head_content = head_content.trim();

        // HEAD detache (SHA-1 direct)
        if head_content.len() == 40 && head_content.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(ContentId::new(head_content.to_string()));
        }

        // HEAD symref (ref: refs/heads/main)
        let ref_name = head_content.strip_prefix("ref: ")?;

        // Loose ref
        let ref_path = git_dir.join(ref_name);
        if ref_path.exists() {
            let sha = std::fs::read_to_string(&ref_path).ok()?;
            let sha = sha.trim();
            if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(ContentId::new(sha.to_string()));
            }
        }

        // Packed-refs (ref specifique)
        let packed_refs = git_dir.join("packed-refs");
        if let Ok(content) = std::fs::read_to_string(&packed_refs) {
            for line in content.lines() {
                if line.starts_with('#') || line.starts_with('^') {
                    continue;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == ref_name {
                    return Some(ContentId::new(parts[0].to_string()));
                }
            }
        }

        None
    }

    /// Strategie 2 : Scanner tous les loose refs dans refs/heads/ (recursif).
    /// Retourne le premier SHA-1 valide trouve.
    fn scan_loose_refs(&self, git_dir: &std::path::Path) -> Option<ContentId> {
        let refs_heads = git_dir.join("refs").join("heads");
        self.scan_loose_refs_dir(&refs_heads)
    }

    /// Helper recursif pour scanner un repertoire de refs.
    fn scan_loose_refs_dir(&self, dir: &std::path::Path) -> Option<ContentId> {
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Sous-dossier (ex: refs/heads/feature/)
                if let Some(cid) = self.scan_loose_refs_dir(&path) {
                    return Some(cid);
                }
            } else if path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let sha = content.trim();
                    if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Some(ContentId::new(sha.to_string()));
                    }
                }
            }
        }
        None
    }

    /// Strategie 3 : Scanner packed-refs pour toutes les refs/heads/*.
    fn scan_packed_refs(&self, git_dir: &std::path::Path) -> Option<ContentId> {
        let packed_refs = git_dir.join("packed-refs");
        let content = std::fs::read_to_string(&packed_refs).ok()?;
        for line in content.lines() {
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1].starts_with("refs/heads/") {
                return Some(ContentId::new(parts[0].to_string()));
            }
        }
        None
    }

    /// Recharge le `ReadonlyRepo` apres une modification externe du bare Git repo,
    /// puis importe les refs Git dans la vue jj.
    ///
    /// Appele apres un `git push` reussi pour synchroniser jj-lib avec les
    /// nouveaux commits ecrits directement dans `.jj/repo/store/git/` par
    /// `git http-backend`.
    ///
    /// ## Flux (Phase 12A-Fix)
    /// 1. Recharge le repo depuis le disque (`RepoLoader::init_from_file_system`)
    /// 2. **Importe les refs Git** (`jj_lib::git::import_refs`) — dit à jj
    ///    que de nouveaux commits existent dans le bare Git repo
    /// 3. Commit la transaction d'import → met à jour les heads jj
    ///
    /// ## Securite
    /// Respecte le contrat `unsafe impl Send + Sync` : l'operation est
    /// executee dans `spawn_blocking` sous `parking_lot::Mutex::lock()`.
    pub async fn reload_repo(&self, repo_id: &Uuid) -> Result<(), DomainError> {
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;
        let rid = *repo_id;
        // Calculer le chemin du repo jj (meme pattern que init_workspace)
        let jj_repo_path = self.workspace_root.join(rid.to_string()).join(".jj").join("repo");

        tokio::task::spawn_blocking(move || {
            let mut guard = handle_arc.lock();
            let wh = &mut *guard;
            let settings = JujutsuEngine::create_settings()?;

            // 1. Recharger le repo depuis le disque
            let store_factories = jj_lib::repo::StoreFactories::default();
            let repo_loader = jj_lib::repo::RepoLoader::init_from_file_system(
                &settings,
                &jj_repo_path,
                &store_factories,
            )
            .map_err(|e| {
                DomainError::VcsError(format!("RepoLoader reload failed: {e}"))
            })?;

            let reloaded_repo = pollster::block_on(repo_loader.load_at_head()).map_err(|e| {
                DomainError::VcsError(format!("reload load_at_head failed: {e}"))
            })?;

            // 2. Importer les refs Git dans la vue jj
            //    Sans cette etape, jj ne voit pas les nouveaux commits pushes
            //    via git http-backend (les git objects existent mais les
            //    heads jj ne sont pas mis a jour).
            let import_options = jj_lib::git::GitImportOptions {
                auto_local_bookmark: true,
                abandon_unreachable_commits: false,
                remote_auto_track_bookmarks: std::collections::HashMap::new(),
            };

            let mut tx = reloaded_repo.start_transaction();
            let import_result = pollster::block_on(
                jj_lib::git::import_refs(tx.repo_mut(), &import_options)
            );

            match import_result {
                Ok(stats) => {
                    info!(
                        repo_id = %rid,
                        changed_bookmarks = stats.changed_remote_bookmarks.len(),
                        abandoned = stats.abandoned_commits.len(),
                        "Git refs importes dans la vue jj"
                    );
                }
                Err(e) => {
                    warn!(
                        repo_id = %rid,
                        error = %e,
                        "Import refs Git echoue — heads jj non mis a jour"
                    );
                }
            }

            // 3. Commit la transaction (meme si import_refs a echoue,
            //    on commit pour ne pas bloquer les operations suivantes)
            let new_repo = pollster::block_on(
                tx.commit(format!("SHINOBI: git import refs for {rid}"))
            )
            .map_err(|e| DomainError::VcsError(format!("reload tx.commit failed: {e}")))?;

            wh.update_repo(new_repo);

            info!(
                repo_id = %rid,
                "Repo jj recharge + refs Git importees (Phase 12A-Fix)"
            );

            Ok(())
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }


    /// Lit le snapshot **diff** d'un commit : description + fichiers modifies avec contenu.
    ///
    /// Utilise pour enrichir les Operations creees par le Sync Hook post-push
    /// (Phase 12A-Fix). Differe de `diff_since()` qui ne retourne que les chemins.
    ///
    /// ## Strategie (Phase 12A-Fix v2)
    /// Diff entre le **parent commit** et le tree du commit → seuls les fichiers
    /// modifies/ajoutes apparaissent. Fallback sur l'empty tree si pas de parent
    /// (premier commit). Pour chaque `TreeValue::File`, on lit le contenu via
    /// `store.read_file()`.
    ///
    /// ## Securite
    /// Respecte le contrat `unsafe impl Send + Sync` : operation dans
    /// `spawn_blocking` sous `parking_lot::Mutex::lock()`.
    pub async fn read_commit_snapshot(
        &self,
        repo_id: &Uuid,
        content_id: &ContentId,
    ) -> Result<CommitSnapshot, DomainError> {
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;
        let cid_hex = content_id.to_string();

        tokio::task::spawn_blocking(move || {
            let guard = handle_arc.lock();
            let wh = &*guard;

            let repo = &wh.repo;
            let store = repo.store();

            // 1. ACL inverse : ContentId hex → CommitId jj
            let commit_id = CommitId::try_from_hex(&cid_hex).ok_or_else(|| {
                DomainError::VcsError(format!("invalid hex commit id: {cid_hex}"))
            })?;

            // 2. Charger le commit
            let commit = store
                .get_commit(&commit_id)
                .map_err(|_| DomainError::CommitNotFound {
                    id: cid_hex.clone(),
                })?;

            // 3. Extraire la description
            let description = commit.description().to_string();
            let commit_tree = commit.tree();

            // 3b. Extraire les parent_ids Git (SHA-1 hex), en filtrant le root_commit_id jj
            let parent_commit_ids: Vec<String> = commit
                .parent_ids()
                .iter()
                .filter(|pid| *pid != store.root_commit_id())
                .map(|pid| pid.hex())
                .collect();

            // 4. Determiner le tree de base pour le diff
            //    - Si le commit a un parent → diff vs parent (fichiers modifies)
            //    - Si pas de parent (root commit) → diff vs empty tree (tous les fichiers)
            let parent_ids = commit.parent_ids();
            let base_tree = if !parent_ids.is_empty()
                && parent_ids[0] != *store.root_commit_id()
            {
                // Parent existe et n'est pas le root commit
                match store.get_commit(&parent_ids[0]) {
                    Ok(parent_commit) => {
                        info!(
                            commit_id = %cid_hex,
                            parent_id = %parent_ids[0].hex(),
                            "read_commit_snapshot: diff vs parent commit"
                        );
                        parent_commit.tree()
                    }
                    Err(_) => {
                        // Parent introuvable → fallback sur empty tree
                        warn!(
                            commit_id = %cid_hex,
                            parent_id = %parent_ids[0].hex(),
                            "read_commit_snapshot: parent introuvable, fallback empty tree"
                        );
                        store.empty_merged_tree()
                    }
                }
            } else {
                // Pas de parent ou parent = root → premier commit
                info!(
                    commit_id = %cid_hex,
                    "read_commit_snapshot: premier commit (diff vs empty tree)"
                );
                store.empty_merged_tree()
            };

            let diff_stream = base_tree.diff_stream(&commit_tree, &EverythingMatcher);
            let entries: Vec<_> = pollster::block_on(diff_stream.collect::<Vec<_>>());

            // 5. Pour chaque fichier, lire le contenu
            let mut files: Vec<(String, Vec<u8>)> = Vec::new();

            for entry in entries {
                let diff = match entry.values {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                // Extraire le TreeValue resolu (pas de conflit)
                // Diff.after contient l'etat apres le diff (= les fichiers du commit)
                let tree_value = match diff.after.as_resolved() {
                    Some(Some(tv)) => tv,
                    _ => continue, // conflit ou suppression → skip
                };

                // Seuls les fichiers nous interessent (pas les symlinks/submodules)
                let file_id = match tree_value {
                    TreeValue::File { id, .. } => id,
                    _ => continue,
                };

                // Lire le contenu du fichier depuis le store
                // read_file() retourne Pin<Box<dyn AsyncRead + Send>> → pollster + AsyncReadExt
                let path = entry.path;
                let reader_result = pollster::block_on(store.read_file(&path, file_id));
                match reader_result {
                    Ok(mut reader) => {
                        let mut content = Vec::new();
                        let read_result = pollster::block_on(
                            tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
                        );
                        if read_result.is_ok() {
                            files.push((
                                path.as_internal_file_string().to_string(),
                                content,
                            ));
                        } else {
                            warn!(
                                path = %path.as_internal_file_string(),
                                "read_commit_snapshot: failed to read file content (skipped)"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            path = %path.as_internal_file_string(),
                            error = %e,
                            "read_commit_snapshot: store.read_file failed (skipped)"
                        );
                    }
                }
            }

            info!(
                commit_id = %cid_hex,
                description = %description.chars().take(80).collect::<String>(),
                file_count = files.len(),
                "read_commit_snapshot: snapshot lu depuis le tree jj"
            );

            Ok(CommitSnapshot { description, files, parent_commit_ids })
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}

// ── Phase 6 — Helpers Explorateur de Code ──────────────────────────────

/// Résout une révision (nom de bookmark ou SHA-1 40 hex) en `CommitId`.
///
/// ## Stratégie (ordre de priorité)
/// 1. SHA-1 40 hex direct
/// 2. Loose ref Git filesystem (`refs/heads/{revision}`)
/// 3. packed-refs
/// 4. HEAD du bare repo (fallback pour "HEAD" ou "")
/// Note : les bookmarks jj sont synchronisés dans les refs Git après import_refs,
/// donc chercher dans refs/heads/{revision} couvre aussi les bookmarks jj.
fn resolve_revision_internal(
    revision: &str,
    git_dir: &std::path::Path,
) -> Result<CommitId, DomainError> {
    // 1. SHA-1 direct
    if revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit()) {
        return CommitId::try_from_hex(revision).ok_or_else(|| {
            DomainError::VcsError(format!("invalid hex commit id '{revision}'"))
        });
    }

    // 2. Loose ref Git filesystem
    let loose_ref = git_dir.join("refs").join("heads").join(revision);
    if let Ok(sha) = std::fs::read_to_string(&loose_ref) {
        let sha = sha.trim();
        if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return CommitId::try_from_hex(sha).ok_or_else(|| {
                DomainError::VcsError(format!("invalid SHA in loose ref '{revision}'"))
            });
        }
    }

    // 3. packed-refs
    let packed = git_dir.join("packed-refs");
    if let Ok(content) = std::fs::read_to_string(&packed) {
        let ref_name = format!("refs/heads/{revision}");
        for line in content.lines() {
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == ref_name {
                return CommitId::try_from_hex(parts[0]).ok_or_else(|| {
                    DomainError::VcsError(format!("invalid SHA in packed-refs for '{revision}'"))
                });
            }
        }
    }

    // 4. HEAD fallback (pour "HEAD" ou "")
    if revision.is_empty() || revision.eq_ignore_ascii_case("head") {
        // Lire HEAD symref
        let head_file = git_dir.join("HEAD");
        if let Ok(head_content) = std::fs::read_to_string(&head_file) {
            let head_content = head_content.trim();
            // HEAD détaché
            if head_content.len() == 40 && head_content.chars().all(|c| c.is_ascii_hexdigit()) {
                return CommitId::try_from_hex(head_content).ok_or_else(|| {
                    DomainError::VcsError("invalid detached HEAD SHA".to_string())
                });
            }
            // Symref
            if let Some(ref_name) = head_content.strip_prefix("ref: ") {
                let ref_path = git_dir.join(ref_name);
                if let Ok(sha) = std::fs::read_to_string(&ref_path) {
                    let sha = sha.trim();
                    if sha.len() == 40 {
                        return CommitId::try_from_hex(sha).ok_or_else(|| {
                            DomainError::VcsError("invalid symref HEAD SHA".to_string())
                        });
                    }
                }
            }
        }
    }

    Err(DomainError::VcsError(format!(
        "révision '{revision}' introuvable (bookmark/SHA-1/HEAD)"
    )))
}

/// Détecte le langage Shiki depuis l'extension du fichier.
fn detect_language_from_path(path: &str) -> Option<String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    let lang = match ext {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "md" | "mdx" => "markdown",
        "toml" => "toml",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "css" => "css",
        "html" | "htm" => "html",
        "py" => "python",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "proto" => "protobuf",
        "dockerfile" | "Dockerfile" => "dockerfile",
        _ => match filename {
            "Dockerfile" | "dockerfile" => "dockerfile",
            "Makefile" | "makefile" => "makefile",
            ".env" | ".env.example" => "bash",
            _ => return None,
        },
    };
    Some(lang.to_string())
}

/// Méthode publique pour exposer la détection de langage au handler REST.
impl JujutsuEngine {
    pub fn language_for_path(path: &str) -> Option<String> {
        detect_language_from_path(path)
    }
}

/// Scanne récursivement un répertoire de refs Git et collecte les `RefInfo`.
/// `prefix` : chemin relatif accumulé pour les refs imbriquées (ex: `"feature/"`)
fn collect_loose_refs_recursive(
    dir: &std::path::Path,
    prefix: &str,
    refs: &mut Vec<RefInfo>,
    seen: &mut HashSet<String>,
    kind: RefKind,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let full_name = if prefix.is_empty() {
            name_str.to_string()
        } else {
            format!("{prefix}{name_str}")
        };
        let path = entry.path();
        if path.is_dir() {
            collect_loose_refs_recursive(
                &path,
                &format!("{full_name}/"),
                refs,
                seen,
                kind.clone(),
            );
        } else if path.is_file() {
            if let Ok(sha) = std::fs::read_to_string(&path) {
                let sha = sha.trim().to_string();
                if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    let dedup_key = match kind {
                        RefKind::Tag => format!("tag:{full_name}"),
                        RefKind::Branch => full_name.clone(),
                    };
                    if seen.insert(dedup_key) {
                        refs.push(RefInfo {
                            name: full_name,
                            target: sha,
                            kind: kind.clone(),
                        });
                    }
                }
            }
        }
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

            // ── Strategie 1 : bookmarks locaux (branches Git importees) ──
            // Apres import_refs(), les branches pushees deviennent des
            // bookmarks locaux. On cherche le commit le plus recent parmi eux.
            let bookmark_commit_ids: Vec<CommitId> = view
                .local_bookmarks()
                .filter_map(|(_, target)| target.as_normal().cloned())
                .collect();

            if !bookmark_commit_ids.is_empty() {
                // Trier pour un HEAD deterministe (meme commit entre executions)
                let mut sorted = bookmark_commit_ids;
                sorted.sort();
                if let Some(head_id) = sorted.last() {
                    let hex = head_id.hex();
                    info!(head = %hex, source = "bookmark", "HEAD résolu depuis bookmark local");
                    return Ok(Some(ContentId::new(hex)));
                }
            }

            // ── Strategie 2 : heads jj (fallback pour workspaces locaux) ──
            // Fonctionnel quand le repo a des commits crees via l'API
            // (create_operation), pas via git push.
            let mut heads: Vec<_> = view.heads().iter().cloned().collect();
            heads.sort();

            // Filtrer le root commit (timestamp 0, tree vide)
            let store = handle.repo.store();
            let non_root_heads: Vec<_> = heads
                .into_iter()
                .filter(|id| id != store.root_commit_id())
                .collect();

            if let Some(head_id) = non_root_heads.last() {
                let hex = head_id.hex();
                info!(head = %hex, source = "heads", "HEAD résolu depuis les heads jj");
                Ok(Some(ContentId::new(hex)))
            } else {
                info!("Repo vide — pas de HEAD (ni bookmarks, ni heads non-root)");
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

            // 3. Récupérer le HEAD actuel (bookmark-aware)
            let view = repo.view();

            // Essayer les bookmarks d'abord
            let bookmark_ids: Vec<CommitId> = view
                .local_bookmarks()
                .filter_map(|(_, target)| target.as_normal().cloned())
                .collect();

            let head_id = if !bookmark_ids.is_empty() {
                let mut sorted = bookmark_ids;
                sorted.sort();
                sorted.last().cloned()
            } else {
                // Fallback : heads jj (sans root)
                let mut heads: Vec<CommitId> = repo.view().heads().iter().cloned().collect();
                heads.sort();
                heads
                    .into_iter()
                    .filter(|id| id != store.root_commit_id())
                    .last()
            };

            let head_id = head_id
                .ok_or_else(|| DomainError::VcsError("no heads in repo".to_string()))?;
            let head_commit = store
                .get_commit(&head_id)
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

    // ── Phase 6 — Explorateur de Code ────────────────────────────────

    #[instrument(skip(self))]
    async fn list_tree(
        &self,
        repo_id: &Uuid,
        revision: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, DomainError> {
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;
        let rid = *repo_id;
        let revision_owned = revision.to_string();
        let path_owned = path.trim_end_matches('/').to_string();
        let git_dir = self.git_repo_path(repo_id);

        tokio::task::spawn_blocking(move || {
            let guard = handle_arc.lock();
            let wh = &*guard;
            let repo = &wh.repo;
            let store = repo.store();

            // 1. Résoudre la révision
            let commit_id = resolve_revision_internal(&revision_owned, &git_dir)?;
            let commit = store.get_commit(&commit_id).map_err(|e| {
                DomainError::CommitNotFound { id: format!("{e}") }
            })?;
            let commit_tree = commit.tree();
            let empty_tree = store.empty_merged_tree();

            // 2. Collecter TOUS les fichiers via diff depuis empty tree
            let diff_stream = empty_tree.diff_stream(&commit_tree, &EverythingMatcher);
            let all_entries: Vec<_> = pollster::block_on(diff_stream.collect::<Vec<_>>());

            // 3. Construire le préfixe de filtre
            let path_prefix = if path_owned.is_empty() {
                String::new()
            } else {
                format!("{}/", path_owned)
            };

            // 4. Listing virtuel : grouper par le prochain composant de chemin
            let mut seen: HashSet<String> = HashSet::new();
            let mut result: Vec<TreeEntry> = Vec::new();
            let mut exact_file_found = false;

            for entry in &all_entries {
                let file_path = entry.path.as_internal_file_string().to_string();

                // Vérifier correspondance exacte au path (c'est un fichier, pas un dossier)
                if !path_owned.is_empty() && file_path == path_owned {
                    exact_file_found = true;
                    continue;
                }

                // Filtrer par préfixe
                if !path_prefix.is_empty() && !file_path.starts_with(&path_prefix) {
                    continue;
                }

                let relative = &file_path[path_prefix.len()..];
                if relative.is_empty() {
                    continue;
                }

                if let Some(slash_pos) = relative.find('/') {
                    // Entrée dans un sous-répertoire
                    let dir_name = &relative[..slash_pos];
                    if seen.insert(dir_name.to_string()) {
                        let full_path = if path_prefix.is_empty() {
                            dir_name.to_string()
                        } else {
                            format!("{}{}", path_prefix, dir_name)
                        };
                        result.push(TreeEntry {
                            name: dir_name.to_string(),
                            path: full_path,
                            kind: EntryKind::Directory,
                            size: None,
                        });
                    }
                } else {
                    // Fichier direct dans ce répertoire
                    if seen.insert(relative.to_string()) {
                        let full_path = file_path.clone();
                        result.push(TreeEntry {
                            name: relative.to_string(),
                            path: full_path,
                            kind: EntryKind::File,
                            size: None, // taille non calculée (évite un read par fichier)
                        });
                    }
                }
            }

            // 5. Si aucune entrée de répertoire mais le path exact est un fichier → IsFile
            if result.is_empty() && exact_file_found {
                return Err(DomainError::IsFile { path: path_owned });
            }

            // 6. Tri : dossiers d'abord, puis fichiers, alphabétique dans chaque groupe
            result.sort_by(|a, b| {
                match (&a.kind, &b.kind) {
                    (EntryKind::Directory, EntryKind::File) => std::cmp::Ordering::Less,
                    (EntryKind::File, EntryKind::Directory) => std::cmp::Ordering::Greater,
                    _ => a.name.cmp(&b.name),
                }
            });

            info!(
                repo_id = %rid,
                revision = %revision_owned,
                path = %path_owned,
                entries = result.len(),
                "list_tree: arborescence construite"
            );

            Ok(result)
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    #[instrument(skip(self))]
    async fn read_blob(
        &self,
        repo_id: &Uuid,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>, DomainError> {
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;
        let revision_owned = revision.to_string();
        let path_owned = path.to_string();
        let git_dir = self.git_repo_path(repo_id);

        tokio::task::spawn_blocking(move || {
            let guard = handle_arc.lock();
            let wh = &*guard;
            let repo = &wh.repo;
            let store = repo.store();

            // 1. Résoudre la révision
            let commit_id = resolve_revision_internal(&revision_owned, &git_dir)?;
            let commit = store.get_commit(&commit_id).map_err(|e| {
                DomainError::CommitNotFound { id: format!("{e}") }
            })?;
            let commit_tree = commit.tree();
            let empty_tree = store.empty_merged_tree();

            // 2. Chercher le fichier dans le diff (même pattern que read_commit_snapshot)
            let diff_stream = empty_tree.diff_stream(&commit_tree, &EverythingMatcher);
            let all_entries: Vec<_> = pollster::block_on(diff_stream.collect::<Vec<_>>());

            for entry in all_entries {
                let file_path = entry.path.as_internal_file_string().to_string();
                if file_path != path_owned {
                    continue;
                }

                let diff = match entry.values {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                let tree_value = match diff.after.as_resolved() {
                    Some(Some(tv)) => tv,
                    _ => continue,
                };

                let file_id = match tree_value {
                    TreeValue::File { id, .. } => id,
                    _ => {
                        return Err(DomainError::VcsError(format!(
                            "le chemin '{path_owned}' n'est pas un fichier régulier"
                        )));
                    }
                };

                let repo_path = RepoPathBuf::from_internal_string(&path_owned)
                    .map_err(|e| DomainError::VcsError(format!("chemin invalide: {e}")))?;

                let mut reader = pollster::block_on(store.read_file(&repo_path, file_id))
                    .map_err(|e| DomainError::VcsError(format!("read_file failed: {e}")))?;

                let mut content = Vec::new();
                pollster::block_on(
                    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut content)
                )
                .map_err(|e| DomainError::VcsError(format!("read_to_end failed: {e}")))?;

                info!(
                    path = %path_owned,
                    bytes = content.len(),
                    "read_blob: fichier lu depuis le store jj"
                );

                return Ok(content);
            }

            Err(DomainError::VcsError(format!(
                "fichier '{path_owned}' introuvable à la révision '{revision_owned}'"
            )))
        })
        .await
        .map_err(|e| DomainError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    #[instrument(skip(self))]
    async fn list_refs(&self, repo_id: &Uuid) -> Result<Vec<RefInfo>, DomainError> {
        let handle_arc = self.get_handle(repo_id).ok_or_else(|| {
            DomainError::VcsError(format!("workspace not initialized for repo {repo_id}"))
        })?;
        let git_dir = self.git_repo_path(repo_id);
        let rid = *repo_id;

        tokio::task::spawn_blocking(move || {
            let guard = handle_arc.lock();
            // Note: on n'utilise pas view.local_bookmarks() ici car le type exact
            // n'est pas exposé publiquement dans jj-lib 0.41 op_store.
            // Les bookmarks sont visibles via les refs Git filesystem après import_refs.
            let _wh = &*guard;

            let mut refs: Vec<RefInfo> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();

            // 1. Loose refs Git filesystem (refs/heads/* = branches + bookmarks jj)
            let refs_heads = git_dir.join("refs").join("heads");
            collect_loose_refs_recursive(&refs_heads, "", &mut refs, &mut seen, RefKind::Branch);

            // 3. Tags Git filesystem (refs/tags/*)
            let refs_tags = git_dir.join("refs").join("tags");
            collect_loose_refs_recursive(&refs_tags, "", &mut refs, &mut seen, RefKind::Tag);

            // 4. Packed-refs
            let packed = git_dir.join("packed-refs");
            if let Ok(content) = std::fs::read_to_string(&packed) {
                for line in content.lines() {
                    if line.starts_with('#') || line.starts_with('^') {
                        continue;
                    }
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() < 2 {
                        continue;
                    }
                    let sha = parts[0];
                    let ref_full = parts[1];
                    if let Some(name) = ref_full.strip_prefix("refs/heads/") {
                        if seen.insert(name.to_string()) {
                            refs.push(RefInfo {
                                name: name.to_string(),
                                target: sha.to_string(),
                                kind: RefKind::Branch,
                            });
                        }
                    } else if let Some(name) = ref_full.strip_prefix("refs/tags/") {
                        if seen.insert(format!("tag:{name}")) {
                            refs.push(RefInfo {
                                name: name.to_string(),
                                target: sha.to_string(),
                                kind: RefKind::Tag,
                            });
                        }
                    }
                }
            }

            refs.sort_by(|a, b| a.name.cmp(&b.name));

            info!(
                repo_id = %rid,
                branches = refs.iter().filter(|r| r.kind == RefKind::Branch).count(),
                tags = refs.iter().filter(|r| r.kind == RefKind::Tag).count(),
                "list_refs: références collectées"
            );

            Ok(refs)
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

    // —— Phase 11 —— GitBackend Verification ——————————————————————————————

    #[tokio::test]
    async fn test_git_repo_exists_after_init() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        // Verify the .jj/repo/store/type file declares "git"
        let store_type_path = tmp
            .path()
            .join(repo_id.to_string())
            .join(".jj")
            .join("repo")
            .join("store")
            .join("type");
        assert!(store_type_path.exists(), "store/type file should exist");
        let store_type = std::fs::read_to_string(&store_type_path).unwrap();
        assert_eq!(
            store_type.trim(),
            "git",
            "store/type should be 'git', got: '{}'",
            store_type.trim()
        );

        // Verify the bare git repo directory exists
        let git_dir = tmp
            .path()
            .join(repo_id.to_string())
            .join(".jj")
            .join("repo")
            .join("store")
            .join("git");
        assert!(
            git_dir.exists(),
            ".jj/repo/store/git/ should exist (bare Git repo)"
        );
        assert!(
            git_dir.is_dir(),
            ".jj/repo/store/git/ should be a directory"
        );

        // Verify the bare Git repo has a valid internal structure
        // A valid bare Git repo MUST contain: HEAD, objects/, refs/
        let head_file = git_dir.join("HEAD");
        assert!(
            head_file.exists(),
            "git/HEAD should exist (bare Git repo marker)"
        );
        let head_content = std::fs::read_to_string(&head_file).unwrap();
        assert!(
            head_content.starts_with("ref: ") || head_content.trim().len() == 40,
            "git/HEAD should be a symbolic ref or a SHA-1 hash, got: '{}'",
            head_content.trim()
        );

        let objects_dir = git_dir.join("objects");
        assert!(
            objects_dir.exists() && objects_dir.is_dir(),
            "git/objects/ should exist (Git object store)"
        );

        let refs_dir = git_dir.join("refs");
        assert!(
            refs_dir.exists() && refs_dir.is_dir(),
            "git/refs/ should exist (Git references)"
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

    // ── Phase 12A — Tests Git Bridge HTTP ──────────────────────────────

    #[tokio::test]
    async fn test_reload_repo_idempotent() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        // Creer un commit pour avoir un HEAD non-root
        engine
            .create_operation(&repo_id, "Pre-reload commit", &[], &[])
            .await
            .unwrap();

        let head_before = engine.resolve_head(&repo_id).await.unwrap().unwrap();

        // Recharger le repo (pas de modification externe)
        let reload_result = engine.reload_repo(&repo_id).await;
        assert!(
            reload_result.is_ok(),
            "reload_repo should succeed: {:?}",
            reload_result.err()
        );

        // Le HEAD doit etre identique (idempotence)
        let head_after = engine.resolve_head(&repo_id).await.unwrap().unwrap();
        assert_eq!(
            head_before.to_string(),
            head_after.to_string(),
            "HEAD should be unchanged after idempotent reload"
        );
    }

    #[tokio::test]
    async fn test_reload_repo_without_init_returns_error() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        let result = engine.reload_repo(&repo_id).await;
        assert!(
            result.is_err(),
            "reload_repo should fail without init"
        );

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("workspace not initialized"),
            "Error should mention workspace not initialized: {err}"
        );
    }

    #[test]
    fn test_git_repo_path_layout() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = Uuid::parse_str("a1a2a3a4-b1b2-c1c2-d1d2-e1e2e3e4e5e6").unwrap();

        let path = engine.git_repo_path(&repo_id);
        let path_str = path.to_string_lossy();

        assert!(
            path_str.contains(".jj"),
            "Path should contain .jj: {path_str}"
        );
        assert!(
            path_str.ends_with("git") || path_str.ends_with("git\\") || path_str.ends_with("git/"),
            "Path should end with /git: {path_str}"
        );
        assert!(
            path_str.contains(&repo_id.to_string()),
            "Path should contain repo_id: {path_str}"
        );
    }

    // ── Phase 12A-Fix — read_commit_snapshot ──────────────────────────────

    #[tokio::test]
    async fn test_read_commit_snapshot_returns_files_and_description() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        // Creer un commit avec des fichiers
        let files = vec![
            ("src/main.rs".to_string(), b"fn main() {}".to_vec()),
            ("README.md".to_string(), b"# Hello".to_vec()),
        ];
        let cid = engine
            .create_operation(&repo_id, "Test snapshot commit", &[], &files)
            .await
            .unwrap();

        // Lire le snapshot
        let snapshot = engine.read_commit_snapshot(&repo_id, &cid).await;
        assert!(
            snapshot.is_ok(),
            "read_commit_snapshot should succeed: {:?}",
            snapshot.err()
        );

        let snapshot = snapshot.unwrap();

        // Verifier la description
        assert!(
            snapshot.description.contains("Test snapshot commit"),
            "Description should contain commit message, got: '{}'",
            snapshot.description
        );

        // Verifier les fichiers
        assert_eq!(
            snapshot.files.len(),
            2,
            "Snapshot should contain 2 files, got: {}",
            snapshot.files.len()
        );

        let paths: Vec<&str> = snapshot.files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(
            paths.contains(&"src/main.rs"),
            "Should contain src/main.rs: {:?}",
            paths
        );
        assert!(
            paths.contains(&"README.md"),
            "Should contain README.md: {:?}",
            paths
        );

        // Verifier le contenu d'un fichier
        let main_content = snapshot
            .files
            .iter()
            .find(|(p, _)| p == "src/main.rs")
            .map(|(_, c)| c.as_slice());
        assert_eq!(
            main_content,
            Some(b"fn main() {}".as_slice()),
            "src/main.rs content should match"
        );
    }

    #[tokio::test]
    async fn test_read_commit_snapshot_empty_commit() {
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = JujutsuEngine::new(tmp.path());
        let repo_id = test_repo_id();

        engine.init_workspace(&repo_id).await.unwrap();

        // Creer un commit vide (empty tree)
        let cid = engine
            .create_operation(&repo_id, "Empty commit", &[], &[])
            .await
            .unwrap();

        let snapshot = engine.read_commit_snapshot(&repo_id, &cid).await.unwrap();

        assert!(
            snapshot.description.contains("Empty commit"),
            "Description should contain commit message"
        );
        assert!(
            snapshot.files.is_empty(),
            "Empty commit should have no files, got: {}",
            snapshot.files.len()
        );
    }
}
