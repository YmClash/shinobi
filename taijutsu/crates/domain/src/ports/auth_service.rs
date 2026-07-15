//! Port: AuthService — Contrat d'authentification (Phase 19A).
//!
//! Ce trait définit le contrat pour le hachage de mots de passe,
//! la génération/vérification de JWT et la gestion des Personal
//! Access Tokens (PAT) pour l'authentification Git HTTP.

use crate::entities::session::AuthClaims;
use crate::errors::DomainError;

/// Contrat d'authentification — hachage, JWT, PAT.
///
/// Implémenté par `JwtAuthService` dans la couche infrastructure.
///
/// ## V1 — Décisions de design :
/// - **JWT 7 jours** : pas de refresh token (DevTools standard)
/// - **PAT Full Access** : le token hérite des droits RBAC de l'acteur
/// - **HS256** : secret symétrique (env `JWT_SECRET`), simple à déployer
pub trait AuthService: Send + Sync {
    /// Hache un mot de passe en clair → hash Argon2 (avec salt aléatoire).
    fn hash_password(&self, raw: &str) -> Result<String, DomainError>;

    /// Vérifie un mot de passe en clair contre un hash Argon2.
    fn verify_password(&self, raw: &str, hash: &str) -> Result<bool, DomainError>;

    /// Génère un JWT signé (HS256) pour un acteur authentifié.
    fn generate_jwt(&self, claims: &AuthClaims) -> Result<String, DomainError>;

    /// Vérifie et décode un JWT → AuthClaims.
    /// Retourne `Unauthorized` si le token est invalide ou expiré.
    fn verify_jwt(&self, token: &str) -> Result<AuthClaims, DomainError>;

    /// Génère un Personal Access Token aléatoire.
    ///
    /// Retourne `(raw_token, sha256_hash)` :
    /// - `raw_token` : affiché **une seule fois** à l'utilisateur
    /// - `sha256_hash` : stocké dans la table `credentials`
    fn generate_pat(&self) -> (String, String);

    /// Vérifie un PAT en clair contre un hash SHA-256 stocké.
    fn verify_pat(&self, raw_token: &str, stored_hash: &str) -> bool;
}
