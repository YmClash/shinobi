//! Entité ContentId — Identifiant de contenu adressable (CID / IPLD).
//!
//! Wrapper typé autour d'un hash de contenu, compatible avec le
//! standard IPLD utilisé par Genjutsu (couche P2P / IPFS).
//! Le CID garantit l'intégrité cryptographique : même contenu → même identifiant.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifiant de contenu adressable par hash (Content Identifier).
///
/// Encapsule un hash sous forme de chaîne, conforme au format CID
/// utilisé par IPFS/IPLD. L'implémentation concrète du hashing
/// est déléguée à la couche infrastructure (Genjutsu).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ContentId(String);

impl ContentId {
    /// Construit un CID à partir d'un hash brut.
    ///
    /// # Invariant
    /// Le hash fourni doit être non-vide. En production, le format
    /// sera validé par l'adaptateur Genjutsu (IPFS).
    pub fn new(hash: impl Into<String>) -> Self {
        let hash = hash.into();
        debug_assert!(!hash.is_empty(), "ContentId ne peut pas être vide");
        Self(hash)
    }

    /// Retourne la représentation brute du hash.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consomme le wrapper et retourne le hash sous-jacent.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ContentId {
    fn from(hash: String) -> Self {
        Self::new(hash)
    }
}

impl From<&str> for ContentId {
    fn from(hash: &str) -> Self {
        Self::new(hash)
    }
}

// ── Tests unitaires (pur domaine, zéro infra) ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_content_id_new_stores_hash() {
        let cid = ContentId::new("QmXy123abc");
        assert_eq!(cid.as_str(), "QmXy123abc");
    }

    #[test]
    fn test_content_id_display() {
        let cid = ContentId::new("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi");
        assert_eq!(
            format!("{cid}"),
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
        );
    }

    #[test]
    fn test_content_id_into_inner() {
        let cid = ContentId::new("QmTest");
        let inner = cid.into_inner();
        assert_eq!(inner, "QmTest");
    }

    #[test]
    fn test_content_id_from_string() {
        let cid: ContentId = String::from("QmFromString").into();
        assert_eq!(cid.as_str(), "QmFromString");
    }

    #[test]
    fn test_content_id_from_str_ref() {
        let cid: ContentId = "QmFromStr".into();
        assert_eq!(cid.as_str(), "QmFromStr");
    }

    #[test]
    fn test_content_id_equality() {
        let a = ContentId::new("QmSame");
        let b = ContentId::new("QmSame");
        let c = ContentId::new("QmDifferent");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_content_id_hash_consistency() {
        let a = ContentId::new("QmHash");
        let b = ContentId::new("QmHash");
        let mut set = HashSet::new();
        set.insert(a);
        // b est égal à a, donc le set ne grandit pas
        assert!(set.contains(&b));
        set.insert(b);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_content_id_clone() {
        let original = ContentId::new("QmClone");
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_content_id_serde_roundtrip() {
        let cid = ContentId::new("QmSerdeTest");
        let json = serde_json::to_string(&cid).unwrap();
        assert_eq!(json, "\"QmSerdeTest\"");
        let deserialized: ContentId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cid);
    }
}
