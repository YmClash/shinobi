//! Adaptateur Auth — JwtAuthService (Phase 19A).
//!
//! Implémentation concrète du port `AuthService` :
//! - **Argon2** pour le hachage des mots de passe (salt aléatoire)
//! - **HS256 + jsonwebtoken** pour les JWT (7 jours V1)
//! - **SHA-256** pour les Personal Access Tokens (PAT)

use domain::entities::session::AuthClaims;
use domain::errors::DomainError;
use domain::ports::auth_service::AuthService;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, Algorithm};
use sha2::{Sha256, Digest};
use tracing::warn;

/// Service d'authentification JWT + Argon2 + PAT.
///
/// Thread-safe et clonable (toutes les clés sont dans des `String`/`Vec<u8>`).
#[derive(Clone)]
pub struct JwtAuthService {
    /// Secret HS256 pour signer les JWT.
    jwt_secret: Vec<u8>,
    /// Durée de validité des JWT en secondes (7 jours par défaut).
    jwt_duration_secs: i64,
}

impl JwtAuthService {
    /// Construit le service avec un secret JWT et une durée de validité.
    ///
    /// # Arguments
    /// - `jwt_secret` : Secret HS256 (env `JWT_SECRET`, ≥32 bytes recommandé)
    /// - `jwt_duration_secs` : Durée de validité en secondes (604800 = 7 jours)
    pub fn new(jwt_secret: &str, jwt_duration_secs: i64) -> Self {
        if jwt_secret.len() < 32 {
            warn!("⚠️  JWT_SECRET fait moins de 32 caractères — sécurité faible !");
        }
        Self {
            jwt_secret: jwt_secret.as_bytes().to_vec(),
            jwt_duration_secs,
        }
    }

    /// Retourne la durée de validité JWT configurée (pour les use cases).
    pub fn jwt_duration_secs(&self) -> i64 {
        self.jwt_duration_secs
    }
}

impl AuthService for JwtAuthService {
    fn hash_password(&self, raw: &str) -> Result<String, DomainError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(raw.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| DomainError::Internal(format!("Password hashing failed: {e}")))
    }

    fn verify_password(&self, raw: &str, hash: &str) -> Result<bool, DomainError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| DomainError::Internal(format!("Invalid password hash format: {e}")))?;

        Ok(Argon2::default()
            .verify_password(raw.as_bytes(), &parsed_hash)
            .is_ok())
    }

    fn generate_jwt(&self, claims: &AuthClaims) -> Result<String, DomainError> {
        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(&self.jwt_secret);

        jsonwebtoken::encode(&header, claims, &key)
            .map_err(|e| DomainError::Internal(format!("JWT encoding failed: {e}")))
    }

    fn verify_jwt(&self, token: &str) -> Result<AuthClaims, DomainError> {
        let key = DecodingKey::from_secret(&self.jwt_secret);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        jsonwebtoken::decode::<AuthClaims>(token, &key, &validation)
            .map(|token_data| token_data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    DomainError::Unauthorized("Token expired".to_string())
                }
                _ => DomainError::Unauthorized(format!("Invalid token: {e}")),
            })
    }

    fn generate_pat(&self) -> (String, String) {
        // Générer 32 bytes aléatoires → format hex (64 chars)
        let bytes: [u8; 32] = rand::random();
        let raw_token = format!("shb_{}", hex::encode(bytes));

        // Hash SHA-256 pour le stockage
        let mut hasher = Sha256::new();
        hasher.update(raw_token.as_bytes());
        let hash = hex::encode(hasher.finalize());

        (raw_token, hash)
    }

    fn verify_pat(&self, raw_token: &str, stored_hash: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(raw_token.as_bytes());
        let computed_hash = hex::encode(hasher.finalize());
        computed_hash == stored_hash
    }

    fn hash_pat_for_lookup(&self, raw_token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(raw_token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::actor::ActorType;
    use uuid::Uuid;

    fn test_service() -> JwtAuthService {
        JwtAuthService::new("test-secret-must-be-at-least-32-bytes-long!", 604800)
    }

    #[test]
    fn test_hash_and_verify_password() {
        let service = test_service();
        let hash = service.hash_password("my-password-123").unwrap();
        assert!(service.verify_password("my-password-123", &hash).unwrap());
        assert!(!service.verify_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn test_generate_and_verify_jwt() {
        let service = test_service();
        let claims = AuthClaims::new(
            Uuid::new_v4(),
            "alice".to_string(),
            ActorType::Human,
            604800,
        );

        let token = service.generate_jwt(&claims).unwrap();
        let decoded = service.verify_jwt(&token).unwrap();

        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.handle, "alice");
    }

    #[test]
    fn test_expired_jwt_rejected() {
        let service = test_service();
        let mut claims = AuthClaims::new(
            Uuid::new_v4(),
            "bob".to_string(),
            ActorType::Human,
            0,
        );
        claims.exp = claims.iat - 120; // 120 seconds in the past (exceeds 60s leeway)

        let token = service.generate_jwt(&claims).unwrap();
        let result = service.verify_jwt(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_and_verify_pat() {
        let service = test_service();
        let (raw_token, hash) = service.generate_pat();

        assert!(raw_token.starts_with("shb_"));
        assert_eq!(hash.len(), 64); // SHA-256 hex
        assert!(service.verify_pat(&raw_token, &hash));
        assert!(!service.verify_pat("wrong-token", &hash));
    }
}
