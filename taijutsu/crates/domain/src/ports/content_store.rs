//! Port: ContentStore — Contrat de stockage distribué de contenu.
//!
//! Ce trait abstrait le stockage adressable par contenu (IPFS / IPLD).
//! L'adaptateur concret sera implémenté dans infrastructure/ (Genjutsu).

use async_trait::async_trait;

use crate::entities::content_id::ContentId;
use crate::errors::DomainError;

/// Contrat de stockage distribué adressable par contenu.
///
/// Sera implémenté par l'adaptateur Genjutsu (IPFS/IPLD)
/// dans une phase ultérieure.
#[async_trait]
pub trait ContentStore: Send + Sync {
    /// Stocke un blob de données et retourne son CID.
    async fn store(&self, data: &[u8]) -> Result<ContentId, DomainError>;

    /// Récupère un blob par son CID.
    async fn retrieve(&self, cid: &ContentId) -> Result<Vec<u8>, DomainError>;

    /// Vérifie l'existence d'un contenu sans le télécharger.
    async fn exists(&self, cid: &ContentId) -> Result<bool, DomainError>;

    /// Épingle un contenu pour empêcher sa collecte par le garbage collector.
    async fn pin(&self, cid: &ContentId) -> Result<(), DomainError>;
}
