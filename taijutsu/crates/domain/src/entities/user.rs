//! Entité User — Représente un acteur du système SHINOBI.
//!
//! Un User peut être un développeur humain (via Makimono)
//! ou un agent IA (consommateur Tensai). Le système ne fait
//! aucune distinction architecturale entre les deux.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Acteur du système — humain ou agent IA.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    /// Identifiant unique.
    pub id: Uuid,

    /// Handle unique (ex: "@tensai-agent", "@dev-alice").
    pub handle: String,

    /// Nom d'affichage lisible.
    pub display_name: String,

    /// Date de création du compte.
    pub created_at: DateTime<Utc>,
}

impl User {
    /// Construit un nouvel utilisateur.
    pub fn new(handle: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            handle: handle.into(),
            display_name: display_name.into(),
            created_at: Utc::now(),
        }
    }
}
