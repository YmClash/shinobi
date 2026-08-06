//! Port: FederationRepository — Contrat de persistence pour la fédération.
//!
//! Ce trait définit le contrat pour stocker et récupérer les données
//! nécessaires à la fédération ActivityPub / ForgeFed (Phase 27 + 27-quater).

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::federation::{FederationKeypair, FederationFollow, FederationActivity, InboxActivity};
use crate::errors::DomainError;

/// Contrat de persistence pour les données de fédération.
///
/// Implémenté par `PostgresFederationRepository` dans la couche infrastructure.
#[async_trait]
pub trait FederationRepository: Send + Sync {
    // ── Keypairs ─────────────────────────────────────────────

    /// Récupère la keypair d'un acteur pour la signature HTTP.
    async fn get_keypair(&self, actor_id: &Uuid) -> Result<Option<FederationKeypair>, DomainError>;

    /// Persiste une nouvelle keypair pour un acteur.
    async fn save_keypair(&self, keypair: &FederationKeypair) -> Result<(), DomainError>;

    // ── Follows ──────────────────────────────────────────────

    /// Enregistre un follow fédéré entrant.
    async fn save_follow(&self, follow: &FederationFollow) -> Result<(), DomainError>;

    /// Supprime un follow fédéré (Undo Follow).
    async fn delete_follow(&self, follower_uri: &str, following_actor_id: &Uuid) -> Result<bool, DomainError>;

    /// Liste les followers fédérés d'un acteur local.
    async fn list_followers(&self, actor_id: &Uuid) -> Result<Vec<FederationFollow>, DomainError>;

    /// Compte les followers fédérés d'un acteur local.
    async fn count_followers(&self, actor_id: &Uuid) -> Result<i64, DomainError>;

    // ── Activities (Outbox) — Phase 27-bis-D ─────────────────

    /// Enregistre une activité sortante dans l'outbox.
    async fn save_activity(&self, activity: &FederationActivity) -> Result<(), DomainError>;

    /// Liste les activités récentes d'un acteur (outbox paginé).
    async fn list_activities(&self, actor_id: &Uuid, limit: i64) -> Result<Vec<FederationActivity>, DomainError>;

    /// Compte les activités d'un acteur.
    async fn count_activities(&self, actor_id: &Uuid) -> Result<i64, DomainError>;

    // ── Inbox (Phase 27-quater) — Transactional Inbox ────────

    /// Enregistre une activité entrante dans l'inbox.
    /// Utilise ON CONFLICT DO NOTHING pour la déduplication par activity ID.
    async fn save_inbox_activity(&self, activity: &InboxActivity) -> Result<(), DomainError>;

    /// Liste les activités entrantes d'un acteur (paginé, tri chronologique desc).
    async fn list_inbox_activities(&self, recipient_id: &Uuid, limit: i64) -> Result<Vec<InboxActivity>, DomainError>;

    /// Compte les activités entrantes d'un acteur.
    async fn count_inbox_activities(&self, recipient_id: &Uuid) -> Result<i64, DomainError>;

    /// Marque une activité inbox comme traitée (processed = true).
    async fn mark_inbox_processed(&self, activity_id: &Uuid) -> Result<(), DomainError>;

    // ── Stats (NodeInfo) ─────────────────────────────────────

    /// Compte le nombre total d'utilisateurs locaux (acteurs humains).
    async fn count_local_users(&self) -> Result<i64, DomainError>;

    /// Compte le nombre total de dépôts publics locaux.
    async fn count_local_repos(&self) -> Result<i64, DomainError>;
}

