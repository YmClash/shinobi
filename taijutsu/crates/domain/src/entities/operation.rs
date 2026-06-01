//! Entité Operation — Représente un changement atomique dans le graphe VCS.
//!
//! Une Operation est l'unité fondamentale du système de versioning.
//! Elle correspond à un commit/changement dans le moteur Jujutsu,
//! enrichi des métadonnées SHINOBI (auteur, CID, parenté).

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

    /// Empreinte du contenu adressé par CID (IPLD).
    pub content_id: ContentId,

    /// Description lisible du changement.
    pub description: String,

    /// Opérations parentes — supporte le merge (0..N parents).
    pub parent_ids: Vec<Uuid>,

    /// Horodatage de création (UTC).
    pub created_at: DateTime<Utc>,
}

impl Operation {
    /// Construit une nouvelle opération avec les métadonnées fournies.
    pub fn new(
        author_id: Uuid,
        content_id: ContentId,
        description: impl Into<String>,
        parent_ids: Vec<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            author_id,
            content_id,
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
}
