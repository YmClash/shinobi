//! Entité Session — Claims JWT décodés (Phase 19A — Auth).
//!
//! Représente l'identité authentifiée extraite d'un JWT valide.
//! Utilisé par les extracteurs Axum (AuthUser, MaybeAuth) pour
//! propager l'identité dans les handlers REST.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entities::actor::ActorType;

/// Claims encodés dans le JWT — identité authentifiée.
///
/// ## Champs
/// - `actor_id` : UUID de l'acteur (clé primaire `actors`)
/// - `handle` : handle unique (pour affichage rapide sans query DB)
/// - `actor_type` : type d'acteur (pour filtrage léger côté handler)
/// - `exp` : Unix timestamp d'expiration (7 jours par défaut V1)
/// - `iat` : Unix timestamp d'émission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    /// Subject — UUID de l'acteur.
    pub sub: Uuid,

    /// Handle unique de l'acteur.
    pub handle: String,

    /// Type d'acteur (human, ai_agent, system).
    pub actor_type: ActorType,

    /// Expiration (Unix timestamp seconds).
    pub exp: i64,

    /// Issued at (Unix timestamp seconds).
    pub iat: i64,
}

impl AuthClaims {
    /// Construit des claims pour un acteur avec une durée de validité.
    pub fn new(actor_id: Uuid, handle: String, actor_type: ActorType, duration_secs: i64) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            sub: actor_id,
            handle,
            actor_type,
            exp: now + duration_secs,
            iat: now,
        }
    }

    /// Raccourci vers l'UUID de l'acteur.
    pub fn actor_id(&self) -> Uuid {
        self.sub
    }

    /// Vérifie si le token est expiré.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.exp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_claims_new() {
        let claims = AuthClaims::new(
            Uuid::new_v4(),
            "alice".to_string(),
            ActorType::Human,
            86400 * 7, // 7 jours
        );
        assert_eq!(claims.handle, "alice");
        assert!(!claims.is_expired());
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_auth_claims_expired() {
        let mut claims = AuthClaims::new(
            Uuid::new_v4(),
            "bob".to_string(),
            ActorType::Human,
            0,
        );
        claims.exp = claims.iat - 10; // 10 seconds in the past
        assert!(claims.is_expired());
    }

    #[test]
    fn test_auth_claims_serde_roundtrip() {
        let claims = AuthClaims::new(
            Uuid::new_v4(),
            "tensai".to_string(),
            ActorType::AiAgent,
            3600,
        );
        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: AuthClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims.sub, deserialized.sub);
        assert_eq!(claims.handle, deserialized.handle);
    }
}
