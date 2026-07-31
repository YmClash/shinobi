//! Entité AnbuCheckpoint — Checkpoint ANBU synchronisé sur le serveur.
//!
//! Représente un snapshot du contexte IA (plans, tâches, walkthroughs,
//! logs) capturé par le CLI ANBU et exfiltré vers IPFS + PostgreSQL.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Un checkpoint ANBU synchronisé depuis le CLI vers le serveur.
///
/// Les artifacts eux-mêmes sont stockés sur IPFS (Genjutsu),
/// référencés par `ipfs_cid` (CID du manifeste DAG racine).
/// PostgreSQL ne stocke que les métadonnées d'indexation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnbuCheckpoint {
    /// Identifiant unique (même UUID que le CLI local).
    pub id: Uuid,
    /// Identifiant du dépôt cible.
    pub repository_id: Uuid,
    /// Identifiant de l'acteur qui a synchronisé.
    pub actor_id: Uuid,
    /// Agent IA source (ex: "antigravity").
    pub agent: String,
    /// Session ID de l'agent (conversation UUID).
    pub session_id: String,
    /// Message utilisateur.
    pub message: Option<String>,
    /// Référence jj/git (change-id ou commit hash).
    pub commit_id: Option<String>,
    /// CID IPFS du manifeste DAG (racine des artifacts).
    pub ipfs_cid: String,
    /// Nombre d'artifacts dans le checkpoint.
    pub artifact_count: i32,
    /// Taille totale des artifacts en bytes.
    pub total_size: i64,
    /// Timestamp de création.
    pub created_at: DateTime<Utc>,
}
