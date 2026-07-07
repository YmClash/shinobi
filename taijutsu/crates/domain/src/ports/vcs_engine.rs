//! Port: VcsEngine — Contrat d'interaction avec le moteur de versioning.
//!
//! Ce trait abstrait le moteur VCS sous-jacent (Jujutsu / jj-lib).
//! L'adaptateur concret dans infrastructure/ implémente un Anti-Corruption Layer
//! pour absorber les évolutions de l'API jj-lib tout en maintenant
//! un contrat stable pour les use cases.
//!
//! ## Multi-Tenant (Phase 10A)
//! Toutes les méthodes prennent un `repo_id: &Uuid` pour identifier
//! le workspace VCS cible. Le `JujutsuEngine` maintient un registre
//! `DashMap<Uuid, Arc<Mutex<WorkspaceHandle>>>` pour la concurrence par-repo.
//!
//! ## Explorer (Phase 6)
//! Trois nouvelles méthodes en lecture seule pour naviguer l'arborescence :
//! `list_tree`, `read_blob`, `list_refs`.

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::content_id::ContentId;
use crate::errors::DomainError;

// ── Types Phase 6 — Explorateur de Code ──────────────────────────────

/// Type d'une entrée dans l'arborescence du dépôt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// Fichier source.
    File,
    /// Répertoire (virtuel — reconstruit depuis la liste plate de jj).
    Directory,
}

/// Entrée d'arborescence : fichier ou répertoire à un chemin donné.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    /// Nom court de l'entrée (ex: `"main.rs"`, `"src"`).
    pub name: String,
    /// Chemin complet relatif à la racine du dépôt (ex: `"src/main.rs"`).
    pub path: String,
    /// Type de l'entrée.
    pub kind: EntryKind,
    /// Taille en octets — `Some` pour les fichiers, `None` pour les dossiers.
    pub size: Option<u64>,
}

/// Type d'une référence Git (branche ou tag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    Tag,
}

/// Référence Git — bookmark jj ou tag.
#[derive(Debug, Clone)]
pub struct RefInfo {
    /// Nom court (ex: `"main"`, `"feature/auth"`, `"v1.0.0"`).
    pub name: String,
    /// SHA-1 du commit pointé (40 hex chars).
    pub target: String,
    /// Type (branche ou tag).
    pub kind: RefKind,
}

// ── Trait VcsEngine ───────────────────────────────────────────────────

/// Contrat d'interaction avec le moteur VCS.
///
/// Conçu comme un Anti-Corruption Layer : l'interface reste stable
/// même si l'API jj-lib évolue entre les versions.
///
/// ## Isolation Multi-Tenant
/// Chaque `repo_id` correspond à un workspace physique isolé
/// dans `{workspace_root}/{repo_id}/.jj`. Les verrous sont par-repo,
/// pas globaux — deux acteurs peuvent commiter dans des dépôts
/// différents en parallèle sans contention.
#[async_trait]
pub trait VcsEngine: Send + Sync {
    /// Initialise un nouveau workspace VCS pour un dépôt donné.
    /// Le chemin physique est dérivé du `repo_id`.
    async fn init_workspace(&self, repo_id: &Uuid) -> Result<(), DomainError>;

    /// Enregistre une opération dans le graphe de versioning d'un dépôt.
    /// Retourne le CID du contenu associé.
    ///
    /// `files` : liste de (chemin relatif, contenu bytes) à écrire dans le tree.
    /// Si la slice est vide, un empty_tree est utilisé (commit de métadonnées).
    async fn create_operation(
        &self,
        repo_id: &Uuid,
        description: &str,
        parent_ids: &[String],
        files: &[(String, Vec<u8>)],
    ) -> Result<ContentId, DomainError>;

    /// Résout la tête courante du graphe (HEAD) d'un dépôt.
    async fn resolve_head(&self, repo_id: &Uuid) -> Result<Option<ContentId>, DomainError>;

    /// Liste les changements depuis une opération donnée dans un dépôt.
    async fn diff_since(&self, repo_id: &Uuid, content_id: &ContentId) -> Result<Vec<String>, DomainError>;

    // ── Phase 6 — Explorateur de Code ────────────────────────────────

    /// Liste les entrées d'un répertoire à une révision donnée.
    ///
    /// `revision` : nom de bookmark (`"main"`) ou SHA-1 40 hex chars.
    /// `path` : chemin relatif (`""` pour la racine, `"src"` pour un sous-dossier).
    ///
    /// ## Comportement
    /// - Si `path` pointe vers un **répertoire** → retourne `Vec<TreeEntry>`
    /// - Si `path` pointe vers un **fichier** → retourne `Err(DomainError::IsFile)`
    ///   (signal au handler pour basculer vers `read_blob`)
    /// - Si `path` n'existe pas → retourne `Err(DomainError::CommitNotFound)`
    ///   ou `Err(DomainError::VcsError)`
    async fn list_tree(
        &self,
        repo_id: &Uuid,
        revision: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, DomainError>;

    /// Retourne le contenu brut d'un fichier à une révision donnée.
    ///
    /// `revision` : nom de bookmark ou SHA-1 40 hex chars.
    /// `path` : chemin complet du fichier (ex: `"src/main.rs"`).
    async fn read_blob(
        &self,
        repo_id: &Uuid,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>, DomainError>;

    /// Liste toutes les branches (bookmarks jj) et tags du dépôt.
    ///
    /// Combine les bookmarks locaux jj (issus de `import_refs` post-push)
    /// et les refs Git du bare repo (loose refs + packed-refs).
    async fn list_refs(&self, repo_id: &Uuid) -> Result<Vec<RefInfo>, DomainError>;
}
