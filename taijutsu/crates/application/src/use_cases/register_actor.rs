//! Use Case: RegisterActor — Inscription d'un nouvel acteur (Phase 19A).
//!
//! Orchestre l'inscription complète :
//! 1. Validation de l'unicité du handle et de l'email
//! 2. Hachage du mot de passe (Argon2)
//! 3. Création de l'acteur en base
//! 4. Stockage du credential (password)
//! 5. Génération d'un JWT (login automatique)

use std::sync::Arc;

use tracing::{info, instrument};

use domain::entities::actor::{Actor, ActorType, is_reserved_handle};
use domain::entities::session::AuthClaims;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::auth_service::AuthService;

// ── Commande ──────────────────────────────────────────────────────────

/// Données nécessaires pour l'inscription d'un nouvel acteur.
#[derive(Debug)]
pub struct RegisterCommand {
    pub handle: String,
    pub display_name: String,
    pub email: String,
    pub password: String,
}

// ── Résultat ──────────────────────────────────────────────────────────

/// Résultat de l'inscription : l'acteur créé + un JWT valide.
#[derive(Debug)]
pub struct RegisterResult {
    pub actor: Actor,
    pub token: String,
}

// ── Use Case ──────────────────────────────────────────────────────────

/// Use case d'inscription d'un nouvel acteur humain.
pub struct RegisterActorUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    auth_service: Arc<dyn AuthService>,
    jwt_duration_secs: i64,
}

impl RegisterActorUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        auth_service: Arc<dyn AuthService>,
        jwt_duration_secs: i64,
    ) -> Self {
        Self {
            actor_repo,
            auth_service,
            jwt_duration_secs,
        }
    }

    #[instrument(skip(self, cmd), fields(handle = %cmd.handle, email = %cmd.email))]
    pub async fn execute(&self, cmd: RegisterCommand) -> Result<RegisterResult, DomainError> {
        // 1. Valider le handle (même règles que le slug de repo)
        validate_handle(&cmd.handle)?;

        // 2. Valider l'email (format basique)
        validate_email(&cmd.email)?;

        // 3. Vérifier l'unicité du handle
        if self.actor_repo.find_by_handle(&cmd.handle).await?.is_some() {
            return Err(DomainError::Duplicate(format!(
                "Handle '{}' déjà pris",
                cmd.handle
            )));
        }

        // 4. Vérifier l'unicité de l'email
        if self.actor_repo.find_by_email(&cmd.email).await?.is_some() {
            return Err(DomainError::Duplicate(format!(
                "Email '{}' déjà utilisé",
                cmd.email
            )));
        }

        // 5. Hacher le mot de passe
        let password_hash = self.auth_service.hash_password(&cmd.password)?;

        // 6. Créer l'acteur
        let mut actor = Actor::new(&cmd.handle, &cmd.display_name, ActorType::Human);
        actor.email = Some(cmd.email.clone());

        self.actor_repo.save(&actor).await?;

        // 7. Stocker le credential (password)
        self.actor_repo
            .save_credential(
                &actor.id,
                "password",
                &password_hash,
                Some(&cmd.email),
                None,
            )
            .await?;

        info!(
            actor_id = %actor.id,
            handle = %actor.handle,
            "✅ Nouvel acteur inscrit — Les Portes de Babylone s'ouvrent"
        );

        // 8. Générer un JWT (auto-login après register)
        let claims = AuthClaims::new(
            actor.id,
            actor.handle.clone(),
            actor.actor_type.clone(),
            self.jwt_duration_secs,
        );
        let token = self.auth_service.generate_jwt(&claims)?;

        Ok(RegisterResult { actor, token })
    }
}

// ── Validation ────────────────────────────────────────────────────────

fn validate_handle(handle: &str) -> Result<(), DomainError> {
    // Phase 27-pre : Guard anti-usurpation — handles réservés au système
    if is_reserved_handle(handle) {
        return Err(DomainError::BusinessRule(
            format!("Le handle '{}' est réservé par le système SHINOBI", handle),
        ));
    }

    if handle.is_empty() || handle.len() > 39 {
        return Err(DomainError::BusinessRule(
            "Le handle doit contenir entre 1 et 39 caractères".to_string(),
        ));
    }

    if !handle
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(DomainError::BusinessRule(
            "Le handle ne peut contenir que des lettres minuscules, chiffres, tirets et underscores"
                .to_string(),
        ));
    }

    if handle.starts_with('-') || handle.ends_with('-') {
        return Err(DomainError::BusinessRule(
            "Le handle ne peut pas commencer ou finir par un tiret".to_string(),
        ));
    }

    Ok(())
}

// ── Tests unitaires (Phase 27-pre) ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reserved_handle_system_rejected() {
        let result = validate_handle("system");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("réservé"), "Expected 'réservé' in: {msg}");
    }

    #[test]
    fn test_reserved_handle_admin_rejected() {
        let result = validate_handle("admin");
        assert!(result.is_err());
    }

    #[test]
    fn test_reserved_handle_oracle_case_insensitive() {
        // is_reserved_handle fait .to_lowercase(), mais validate_handle
        // rejette aussi les majuscules via la regex → double protection.
        let result = validate_handle("oracle");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_handle_passes() {
        assert!(validate_handle("ymclash").is_ok());
        assert!(validate_handle("alice-dev").is_ok());
        assert!(validate_handle("ninja_42").is_ok());
    }

    #[test]
    fn test_handle_format_validation() {
        // Trop long
        assert!(validate_handle(&"a".repeat(40)).is_err());
        // Vide
        assert!(validate_handle("").is_err());
        // Commence par tiret
        assert!(validate_handle("-invalid").is_err());
        // Finit par tiret
        assert!(validate_handle("invalid-").is_err());
    }

    #[test]
    fn test_new_reserved_handles_phase27() {
        assert!(validate_handle("noreply").is_err());
        assert!(validate_handle("security").is_err());
        assert!(validate_handle("administrator").is_err());
        assert!(validate_handle("abuse").is_err());
        assert!(validate_handle("postmaster").is_err());
        assert!(validate_handle("webmaster").is_err());
    }
}

fn validate_email(email: &str) -> Result<(), DomainError> {
    if !email.contains('@') || email.len() < 5 {
        return Err(DomainError::BusinessRule(
            "Format d'email invalide".to_string(),
        ));
    }
    Ok(())
}
