//! Use Case: CreatePat — Création d'un Personal Access Token (Phase 19A).
//!
//! Flow :
//! 1. Vérifier que l'acteur existe
//! 2. Générer un PAT aléatoire (raw + SHA-256 hash)
//! 3. Stocker le hash dans credentials (type api_key)
//! 4. Retourner le token en clair (unique affichage)

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::actor_repository::{ActorRepository, PatInfo};
use domain::ports::auth_service::AuthService;

// ── Commande ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CreatePatCommand {
    pub actor_id: Uuid,
    pub label: Option<String>,
}

// ── Résultat ──────────────────────────────────────────────────────────

/// Le token en clair — affiché **une seule fois** à l'utilisateur.
#[derive(Debug)]
pub struct CreatePatResult {
    /// Token en clair (format: `shb_<64 hex chars>`).
    /// L'utilisateur doit le copier maintenant — il ne sera plus jamais affiché.
    pub raw_token: String,
    /// Label optionnel pour identifier le token.
    pub label: Option<String>,
}

// ── Use Case ──────────────────────────────────────────────────────────

pub struct CreatePatUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    auth_service: Arc<dyn AuthService>,
}

impl CreatePatUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        auth_service: Arc<dyn AuthService>,
    ) -> Self {
        Self {
            actor_repo,
            auth_service,
        }
    }

    #[instrument(skip(self), fields(actor_id = %cmd.actor_id))]
    pub async fn execute(&self, cmd: CreatePatCommand) -> Result<CreatePatResult, DomainError> {
        // 1. Vérifier que l'acteur existe
        let actor = self
            .actor_repo
            .find_by_id(&cmd.actor_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: cmd.actor_id,
            })?;

        // 2. Générer le PAT
        let (raw_token, hash) = self.auth_service.generate_pat();

        // 3. Stocker le hash dans credentials
        self.actor_repo
            .save_credential(
                &actor.id,
                "api_key",
                &hash,
                None,
                cmd.label.as_deref(),
            )
            .await?;

        info!(
            actor_id = %actor.id,
            handle = %actor.handle,
            label = ?cmd.label,
            "✅ PAT créé — ne sera plus affiché après cette réponse"
        );

        Ok(CreatePatResult {
            raw_token,
            label: cmd.label,
        })
    }

    /// Liste les PAT d'un acteur (métadonnées uniquement, pas les secrets).
    #[instrument(skip(self))]
    pub async fn list(&self, actor_id: &Uuid) -> Result<Vec<PatInfo>, DomainError> {
        self.actor_repo.list_pats(actor_id).await
    }
}
