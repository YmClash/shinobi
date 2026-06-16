//! Entité Operation — Représente un changement atomique dans le graphe VCS.
//!
//! Une Operation est l'unité fondamentale du système de versioning.
//! Elle correspond à un commit/changement dans le moteur Jujutsu,
//! enrichi des métadonnées SHINOBI (auteur, CID, parenté).
//!
//! ## Double CID (Phase 5)
//! - `content_id` : CID interne jj-lib (hash du commit Jujutsu).
//! - `ipfs_content_id` : CID IPFS distribué (optionnel, Genjutsu).
//! La synchronisation jj ↔ IPFS est le cœur de Phase 5.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::content_id::ContentId;

/// Changement atomique dans le graphe de versioning.
///
/// Immuable par conception : une fois créée, une opération ne change jamais.
/// Les corrections se font par de nouvelles opérations pointant vers les parentes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Operation {
    /// Identifiant unique de l'opération.
    pub id: Uuid,

    /// Identifiant de l'auteur (humain ou agent IA).
    pub author_id: Uuid,

    /// Empreinte du contenu adressé par CID jj-lib (hash du commit Jujutsu).
    pub content_id: ContentId,

    /// CID IPFS distribué (Genjutsu) — optionnel.
    /// Présent uniquement si le contenu a été synchronisé sur IPFS.
    /// `None` si IPFS était indisponible lors de la création.
    pub ipfs_content_id: Option<ContentId>,

    /// Description lisible du changement.
    pub description: String,

    /// Opérations parentes — supporte le merge (0..N parents).
    pub parent_ids: Vec<Uuid>,

    /// Horodatage de création (UTC).
    pub created_at: DateTime<Utc>,
}

impl Operation {
    /// Construit une nouvelle opération avec les métadonnées fournies.
    ///
    /// `ipfs_content_id` est optionnel : `None` si IPFS est indisponible
    /// ou si le contenu n'a pas été synchronisé.
    pub fn new(
        author_id: Uuid,
        content_id: ContentId,
        ipfs_content_id: Option<ContentId>,
        description: impl Into<String>,
        parent_ids: Vec<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            author_id,
            content_id,
            ipfs_content_id,
            description: description.into(),
            parent_ids,
            created_at: Utc::now(),
        }
    }

    /// Vérifie si cette opération est une racine (aucun parent).
    pub fn is_root(&self) -> bool {
        self.parent_ids.is_empty()
    }

    /// Vérifie si cette opération est un merge (plus d'un parent).
    pub fn is_merge(&self) -> bool {
        self.parent_ids.len() > 1
    }

    /// Vérifie si le contenu a été synchronisé sur IPFS (Genjutsu).
    pub fn has_ipfs_content(&self) -> bool {
        self.ipfs_content_id.is_some()
    }
}

// ── Tests unitaires (pur domaine, zéro infra) ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_author_id() -> Uuid {
        Uuid::parse_str("a1a2a3a4-b1b2-c1c2-d1d2-e1e2e3e4e5e6").unwrap()
    }

    fn test_parent_id() -> Uuid {
        Uuid::parse_str("f1f2f3f4-a1a2-b1b2-c1c2-d1d2d3d4d5d6").unwrap()
    }

    #[test]
    fn test_operation_new_generates_unique_id() {
        let op1 = Operation::new(
            test_author_id(),
            ContentId::new("QmOp1"),
            None,
            "premier",
            vec![],
        );
        let op2 = Operation::new(
            test_author_id(),
            ContentId::new("QmOp2"),
            None,
            "deuxième",
            vec![],
        );
        assert_ne!(op1.id, op2.id, "Chaque opération doit avoir un UUID unique");
    }

    #[test]
    fn test_operation_is_root_with_no_parents() {
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmRoot"),
            None,
            "racine",
            vec![],
        );
        assert!(op.is_root());
        assert!(!op.is_merge());
    }

    #[test]
    fn test_operation_is_not_root_with_parent() {
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmChild"),
            None,
            "enfant",
            vec![test_parent_id()],
        );
        assert!(!op.is_root());
        assert!(!op.is_merge());
    }

    #[test]
    fn test_operation_is_merge_with_two_parents() {
        let parent_a = Uuid::new_v4();
        let parent_b = Uuid::new_v4();
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmMerge"),
            None,
            "merge commit",
            vec![parent_a, parent_b],
        );
        assert!(!op.is_root());
        assert!(op.is_merge());
    }

    #[test]
    fn test_operation_without_ipfs_content() {
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmJjOnly"),
            None,
            "sans IPFS",
            vec![],
        );
        assert!(!op.has_ipfs_content());
        assert!(op.ipfs_content_id.is_none());
    }

    #[test]
    fn test_operation_with_ipfs_content() {
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmJjCid"),
            Some(ContentId::new("QmIpfsCid")),
            "avec IPFS",
            vec![],
        );
        assert!(op.has_ipfs_content());
        assert_eq!(op.ipfs_content_id.unwrap().as_str(), "QmIpfsCid");
    }

    #[test]
    fn test_operation_dual_cid_independence() {
        let jj_cid = ContentId::new("QmJjHash123");
        let ipfs_cid = ContentId::new("bafybeigIPFSHash456");
        let op = Operation::new(
            test_author_id(),
            jj_cid.clone(),
            Some(ipfs_cid.clone()),
            "dual CID",
            vec![],
        );
        assert_eq!(op.content_id, jj_cid);
        assert_eq!(op.ipfs_content_id.as_ref().unwrap(), &ipfs_cid);
        assert_ne!(
            op.content_id.as_str(),
            op.ipfs_content_id.as_ref().unwrap().as_str()
        );
    }

    #[test]
    fn test_operation_stores_description() {
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmDesc"),
            None,
            "Fix critical bug in auth module",
            vec![],
        );
        assert_eq!(op.description, "Fix critical bug in auth module");
    }

    #[test]
    fn test_operation_serde_roundtrip() {
        let op = Operation::new(
            test_author_id(),
            ContentId::new("QmSerde"),
            Some(ContentId::new("QmIpfsSerde")),
            "test serde",
            vec![test_parent_id()],
        );
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Operation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, deserialized);
    }
}
