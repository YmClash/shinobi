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
    /// Le chemin physique est dérivé de `owner_id` et `repo_id` :
    /// `{workspace_root}/{owner_id}/{repo_id}/`
    ///
    /// ## Multi-Tenant (Phase 21)
    /// L'`owner_id` est utilisé pour l'isolation physique par propriétaire.
    /// Les UUIDs sont utilisés (pas les handles) pour éviter les migrations
    /// physiques lors des renommages de comptes.
    async fn init_workspace(&self, owner_id: &Uuid, repo_id: &Uuid) -> Result<(), DomainError>;

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

    // ── Phase 17 — Diff Colorisé ────────────────────────────────────

    /// Calcule le diff ligne par ligne entre un commit et son parent.
    ///
    /// Pour chaque fichier modifié, retourne les hunks avec les lignes
    /// `Add`, `Remove` et `Context` — prêts pour l'affichage GitHub-style.
    ///
    /// Utilise la crate `similar` pour le calcul du diff textuel.
    ///
    /// ## Comportement
    /// - Commit avec parent → diff vs parent
    /// - Commit sans parent (root) → diff vs empty tree (tous les fichiers = Added)
    /// - Fichiers binaires → status `Binary`, pas de hunks
    /// - Fichiers >1000 lignes de diff → flag `too_large: true`
    async fn diff_content(
        &self,
        repo_id: &Uuid,
        content_id: &ContentId,
    ) -> Result<Vec<FileDiff>, DomainError>;
}

// ── Phase 17 — Types de Diff Colorisé ────────────────────────────────

/// Status d'un fichier dans le diff.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffStatus {
    /// Fichier ajouté (n'existait pas dans le parent).
    Added,
    /// Fichier modifié (contenu différent du parent).
    Modified,
    /// Fichier supprimé (présent dans le parent, absent dans le commit).
    Deleted,
}

/// Type d'une ligne dans un hunk de diff.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    /// Ligne ajoutée (+).
    Add,
    /// Ligne supprimée (-).
    Remove,
    /// Ligne de contexte (inchangée).
    Context,
}

/// Une ligne individuelle dans un hunk de diff.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffLine {
    /// Type de la ligne (add, remove, context).
    pub kind: DiffLineKind,
    /// Contenu textuel de la ligne.
    pub content: String,
    /// Numéro de ligne dans le fichier original (avant). `None` pour les lignes ajoutées.
    pub old_line: Option<u32>,
    /// Numéro de ligne dans le fichier modifié (après). `None` pour les lignes supprimées.
    pub new_line: Option<u32>,
}

/// Un bloc contigu de changements dans un fichier.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffHunk {
    /// En-tête du hunk (ex: `"@@ -1,5 +1,7 @@"`).
    pub header: String,
    /// Lignes du hunk (add, remove, context).
    pub lines: Vec<DiffLine>,
}

/// Diff complet d'un fichier individuel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileDiff {
    /// Chemin du fichier (relatif à la racine du dépôt).
    pub path: String,
    /// Status du fichier dans le diff (added, modified, deleted).
    pub status: DiffStatus,
    /// Hunks de diff (blocs de changements).
    /// Vide si `too_large` est `true` ou si le fichier est binaire.
    pub hunks: Vec<DiffHunk>,
    /// Nombre de lignes ajoutées.
    pub additions: u32,
    /// Nombre de lignes supprimées.
    pub deletions: u32,
    /// `true` si le diff dépasse 1000 lignes — protection du DOM navigateur.
    pub too_large: bool,
}

