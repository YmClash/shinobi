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

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::content_id::ContentId;
use crate::errors::DomainError;

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
}
