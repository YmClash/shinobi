//! Use Case: LoginActor — Connexion d'un acteur existant (Phase 19A).
//!
//! Flow :
//! 1. Retrouver l'acteur par email
//! 2. Retrouver le credential de type password
//! 3. Vérifier le mot de passe (Argon2)
//! 4. Générer un JWT (7 jours V1)

use std::sync::Arc;

use tracing::{info, instrument, warn};

use domain::entities::actor::Actor;
use domain::entities::session::AuthClaims;
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::auth_service::AuthService;

// ── Commande ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct LoginCommand {
    pub email: String,
    pub password: String,
}

// ── Résultat ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct LoginResult {
    pub actor: Actor,
    pub token: String,
}

// ── Use Case ──────────────────────────────────────────────────────────

pub struct LoginActorUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    auth_service: Arc<dyn AuthService>,
    jwt_duration_secs: i64,
}

impl LoginActorUseCase {
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

    #[instrument(skip(self, cmd), fields(email = %cmd.email))]
    pub async fn execute(&self, cmd: LoginCommand) -> Result<LoginResult, DomainError> {
        // 1. Retrouver l'acteur par email
        let actor = self
            .actor_repo
            .find_by_email(&cmd.email)
            .await?
            .ok_or_else(|| {
                warn!(email = %cmd.email, "Login failed — email not found");
                DomainError::Unauthorized("Identifiants invalides".to_string())
            })?;

        // Phase 27-pre : Defense in Depth — l'acteur système ne peut jamais se connecter
        if actor.is_system() {
            warn!(actor_id = %actor.id, "🚫 Login rejeté — tentative de connexion sur l'acteur système");
            return Err(DomainError::Unauthorized(
                "Identifiants invalides".to_string(),
            ));
        }

        // 2. Retrouver le hash du mot de passe
        let password_hash = self
            .actor_repo
            .find_credential_hash(&actor.id, "password")
            .await?
            .ok_or_else(|| {
                warn!(actor_id = %actor.id, "Login failed — no password credential");
                DomainError::Unauthorized("Identifiants invalides".to_string())
            })?;

        // 3. Vérifier le mot de passe
        let valid = self
            .auth_service
            .verify_password(&cmd.password, &password_hash)?;

        if !valid {
            warn!(actor_id = %actor.id, "Login failed — wrong password");
            return Err(DomainError::Unauthorized(
                "Identifiants invalides".to_string(),
            ));
        }

        // 4. Générer le JWT
        let claims = AuthClaims::new(
            actor.id,
            actor.handle.clone(),
            actor.actor_type.clone(),
            self.jwt_duration_secs,
        );
        let token = self.auth_service.generate_jwt(&claims)?;

        info!(
            actor_id = %actor.id,
            handle = %actor.handle,
            "✅ Login réussi — Bienvenue dans la Forge"
        );

        Ok(LoginResult { actor, token })
    }
}
