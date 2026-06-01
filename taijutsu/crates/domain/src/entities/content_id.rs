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
