//! Entités de fédération — Phase 27 ForgeFed.
//!
//! Représente les données cryptographiques et protocolaires nécessaires
//! à la fédération ActivityPub / ForgeFed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Keypair fédérée ──────────────────────────────────────────────────

/// Paire de clés RSA-2048 associée à un acteur pour la fédération.
///
/// Utilisée pour :
/// - Signer les requêtes HTTP sortantes (Draft-Cavage-12)
/// - Exposer la clé publique dans le profil ActivityPub (`publicKey`)
/// - Vérifier les requêtes HTTP entrantes (Inbox)
///
/// Stockée dans PostgreSQL (table `federation_keys`) pour la portabilité Docker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationKeypair {
    /// ID de l'acteur propriétaire de la clé.
    pub actor_id: Uuid,

    /// Clé publique au format PEM (RSA-2048).
    pub public_key_pem: String,

    /// Clé privée au format PEM (RSA-2048, PKCS#8).
    /// ⚠️ Ne jamais exposer via l'API publique.
    pub private_key_pem: String,

    /// Identifiant de la clé dans le protocole ActivityPub.
    /// Format : `https://{domain}/actors/{handle}#main-key`
    pub key_id: String,

    /// Date de création de la keypair.
    pub created_at: DateTime<Utc>,
}

// ── Follow fédéré ────────────────────────────────────────────────────

/// Relation de suivi fédéré entre un acteur distant et un acteur local.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationFollow {
    /// Identifiant unique de la relation.
    pub id: Uuid,

    /// URI ActivityPub du follower distant.
    /// Ex: `https://forgejo.example.com/users/alice`
    pub follower_uri: String,

    /// ID de l'acteur local suivi.
    pub following_actor_id: Uuid,

    /// Indique si le Follow a été accepté.
    pub accepted: bool,

    /// Date de création.
    pub created_at: DateTime<Utc>,
}

// ── Activité fédérée (Outbox) ────────────────────────────────────────

/// Activité ActivityPub stockée pour l'Outbox.
///
/// Phase 27-bis-D — Le corps de l'activité est stocké en JSONB
/// pour le polymorphisme natif d'ActivityPub (Create, Update, Delete, Accept).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationActivity {
    /// Identifiant unique de l'activité.
    pub id: Uuid,

    /// ID de l'acteur local auteur de l'activité.
    pub actor_id: Uuid,

    /// Type d'activité ActivityPub (ex: "Create", "Accept", "Update", "Delete").
    pub activity_type: String,

    /// Type de l'objet (ex: "Repository", "Follow", "Note").
    pub object_type: String,

    /// URI de l'objet référencé.
    pub object_id: String,

    /// Activité AP complète en JSON (polymorphique).
    pub activity_json: serde_json::Value,

    /// Date de publication.
    pub published_at: DateTime<Utc>,
}

// ── Activité fédérée entrante (Inbox) ──────────────────────────────

/// Activité ActivityPub reçue d'une forge distante.
///
/// Phase 27-quater — Pattern Transactional Inbox :
/// - Insérée avec `processed = false` à la réception
/// - Un worker futur passera `processed = true` après side-effects
///
/// Les champs `object_type` et `object_uri` sont des colonnes matérialisées
/// extraites du JSONB à l'insertion pour des requêtes efficaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxActivity {
    /// Identifiant unique.
    pub id: Uuid,

    /// ID de l'acteur local destinataire.
    pub recipient_actor_id: Uuid,

    /// URI de l'acteur distant expéditeur.
    pub remote_actor_uri: String,

    /// Type d'activité AP (ex: "Create", "Push", "Update", "Delete", "Announce").
    pub activity_type: String,

    /// Type de l'objet référencé (colonne matérialisée).
    pub object_type: String,

    /// URI de l'objet référencé (colonne matérialisée).
    pub object_uri: String,

    /// Activité AP complète en JSON-LD (source de vérité).
    pub activity_json: serde_json::Value,

    /// TRUE si les side-effects métier ont été appliqués.
    pub processed: bool,

    /// Date de réception.
    pub received_at: DateTime<Utc>,

    /// Date de traitement (si processed = true).
    pub processed_at: Option<DateTime<Utc>>,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_keypair_fields() {
        let kp = FederationKeypair {
            actor_id: Uuid::new_v4(),
            public_key_pem: "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----".into(),
            private_key_pem: "-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----".into(),
            key_id: "https://shinobi.example.com/actors/test#main-key".into(),
            created_at: Utc::now(),
        };
        assert!(kp.key_id.contains("#main-key"));
    }

    #[test]
    fn test_federation_follow_default() {
        let follow = FederationFollow {
            id: Uuid::new_v4(),
            follower_uri: "https://mastodon.social/users/bob".into(),
            following_actor_id: Uuid::new_v4(),
            accepted: false,
            created_at: Utc::now(),
        };
        assert!(!follow.accepted);
    }

    #[test]
    fn test_federation_activity_creation() {
        let activity = FederationActivity {
            id: Uuid::new_v4(),
            actor_id: Uuid::new_v4(),
            activity_type: "Create".into(),
            object_type: "Repository".into(),
            object_id: "https://shinobi.dev/repos/ymclash/my-repo".into(),
            activity_json: serde_json::json!({"type": "Create"}),
            published_at: Utc::now(),
        };
        assert_eq!(activity.activity_type, "Create");
        assert_eq!(activity.object_type, "Repository");
    }

    #[test]
    fn test_inbox_activity_creation() {
        let inbox = InboxActivity {
            id: Uuid::new_v4(),
            recipient_actor_id: Uuid::new_v4(),
            remote_actor_uri: "https://forgejo.example.com/users/alice".into(),
            activity_type: "Push".into(),
            object_type: "Repository".into(),
            object_uri: "https://forgejo.example.com/repos/alice/my-lib".into(),
            activity_json: serde_json::json!({"type": "Push"}),
            processed: false,
            received_at: Utc::now(),
            processed_at: None,
        };
        assert_eq!(inbox.activity_type, "Push");
        assert!(!inbox.processed);
        assert!(inbox.processed_at.is_none());
    }
}
