//! Port: ContentStore — Contrat de stockage distribué de contenu.
//!
//! Ce trait abstrait le stockage adressable par contenu (IPFS / IPLD).
//! L'adaptateur concret sera implémenté dans infrastructure/ (Genjutsu).
//!
//! ## Phase 8.1 — Merkle DAG IPLD
//! Ajout de `store_dag()` pour stocker chaque fichier comme un nœud IPFS
//! indépendant (raw bytes) relié par un manifeste racine (DAG-JSON).
//! La détection du format (DAG vs Legacy Blob) à la lecture se fait dans
//! la couche Application via le **Type Tag** pattern (pas de `retrieve_dag()`
//! sur le port — c'est intentionnel).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::entities::content_id::ContentId;
use crate::errors::DomainError;

// ── Types Merkle DAG (Phase 8.1) ─────────────────────────────────────

/// Lien vers un fichier dans le Merkle DAG IPFS.
///
/// Chaque fichier du commit est un nœud IPFS indépendant,
/// identifié par son propre CID. IPFS déduplique automatiquement
/// les fichiers identiques entre commits (même CID = même bloc).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DagFileLink {
    /// Chemin du fichier dans le commit (ex: `"src/main.rs"`).
    pub path: String,
    /// CID IPFS du fichier (ex: `"QmFile1..."` ou `"bafk..."`).
    pub cid: String,
    /// Taille du fichier en bytes (raw, pas base64).
    pub size: usize,
}

/// Manifeste racine d'un Merkle DAG IPLD.
///
/// Ce nœud JSON est stocké sur IPFS et pointe vers tous les fichiers
/// du commit via leurs CIDs. C'est le seul nœud qu'on pin — IPFS
/// résout récursivement les blocs référencés.
///
/// ## Type Tag
/// Le champ `version` sert de discriminant pour distinguer ce format
/// du legacy blob JSON (array de `{path, content_b64, size}`).
/// Détection : `json["version"].is_some() && json["files"].is_some()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DagManifest {
    /// Version du format (toujours `1` pour l'instant).
    pub version: u8,
    /// Description du commit (métadonnée contextuelle).
    pub description: String,
    /// Liens vers les fichiers individuels.
    pub files: Vec<DagFileLink>,
}

// ── Trait ContentStore ────────────────────────────────────────────────

/// Contrat de stockage distribué adressable par contenu.
///
/// Implémenté par l'adaptateur Genjutsu (IPFS/Kubo).
///
/// ## Méthodes existantes (Phase 4C)
/// - `store()` / `retrieve()` : blob brut unique
/// - `exists()` / `pin()` : vérification et épinglage
///
/// ## Phase 8.1 — Merkle DAG
/// - `store_dag()` : stocke chaque fichier comme un nœud IPFS indépendant
///   et retourne le CID du manifeste racine.
///
/// La lecture dual-format (DAG vs Legacy Blob) se fait dans la couche
/// Application via `retrieve()` + inspection `serde_json::Value` (Type Tag).
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

    /// Stocke un ensemble de fichiers comme un Merkle DAG IPLD.
    ///
    /// ## Flux
    /// 1. Chaque fichier est stocké comme un nœud IPFS indépendant (raw bytes)
    /// 2. Un manifeste DAG-JSON est construit avec les CIDs des fichiers
    /// 3. Le manifeste est stocké sur IPFS
    /// 4. Le CID du manifeste (racine) est retourné
    ///
    /// ## Déduplication
    /// Si un fichier identique est déjà sur IPFS (même contenu = même CID),
    /// IPFS ne le re-stocke pas — déduplication automatique au niveau bloc.
    async fn store_dag(
        &self,
        description: &str,
        files: &[(String, Vec<u8>)],
    ) -> Result<(ContentId, DagManifest), DomainError>;
}
