//! Port: VcsEngine — Contrat d'interaction avec le moteur de versioning.
//!
//! Ce trait abstrait le moteur VCS sous-jacent (Jujutsu / jj-lib).
//! L'adaptateur concret dans infrastructure/ implémente un Anti-Corruption Layer
//! pour absorber les évolutions de l'API jj-lib tout en maintenant
//! un contrat stable pour les use cases.

use async_trait::async_trait;

use crate::entities::content_id::ContentId;
use crate::errors::DomainError;

/// Contrat d'interaction avec le moteur VCS.
///
/// Conçu comme un Anti-Corruption Layer : l'interface reste stable
/// même si l'API jj-lib évolue entre les versions.
#[async_trait]
pub trait VcsEngine: Send + Sync {
    /// Initialise un nouveau workspace VCS dans le répertoire cible.
    async fn init_workspace(&self, path: &str) -> Result<(), DomainError>;

    /// Enregistre une opération dans le graphe de versioning.
    /// Retourne le CID du contenu associé.
    ///
    /// `files` : liste de (chemin relatif, contenu bytes) à écrire dans le tree.
    /// Si la slice est vide, un empty_tree est utilisé (commit de métadonnées).
    async fn create_operation(
        &self,
        description: &str,
        parent_ids: &[String],
        files: &[(String, Vec<u8>)],
    ) -> Result<ContentId, DomainError>;

    /// Résout la tête courante du graphe (HEAD).
    async fn resolve_head(&self) -> Result<Option<ContentId>, DomainError>;

    /// Liste les changements depuis une opération donnée.
    async fn diff_since(&self, content_id: &ContentId) -> Result<Vec<String>, DomainError>;
}
