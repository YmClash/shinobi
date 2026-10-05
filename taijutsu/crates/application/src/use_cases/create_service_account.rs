//! Use Case: CreateServiceAccount — Provisionnement d'un Service Account IA (Phase 25).
//!
//! ## Flow
//! 1. Vérifier que le parent existe et est de type Human
//! 2. Valider le handle (mêmes règles que `register_actor`)
//! 3. Vérifier la liste noire de noms réservés
//! 4. Vérifier l'unicité du handle dans le namespace universel
//! 5. Créer l'Actor avec actor_type = AiAgent, parent_id = parent_actor_id
//! 6. Générer un PAT (auth_service.generate_pat())
//! 7. Stocker le hash dans credentials (type api_key)
//! 8. Retourner le bot + token en clair (unique affichage)

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::actor::{Actor, is_reserved_handle};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::auth_service::AuthService;

// ── Commande ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CreateServiceAccountCommand {
    /// L'humain créateur (extrait du JWT).
    pub parent_actor_id: Uuid,
    /// Handle unique du bot (ex: "ymclash-oracle-bot").
    pub handle: String,
    /// Nom d'affichage (ex: "Oracle Bot de ymclash").
    pub display_name: String,
    /// Label optionnel du PAT initial.
    pub label: Option<String>,
}

// ── Résultat ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CreateServiceAccountResult {
    /// Le bot créé.
    pub actor: Actor,
    /// PAT en clair — affiché **une seule fois** à l'utilisateur.
    pub raw_token: String,
}

// ── Use Case ──────────────────────────────────────────────────────────

pub struct CreateServiceAccountUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    auth_service: Arc<dyn AuthService>,
}

impl CreateServiceAccountUseCase {
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        auth_service: Arc<dyn AuthService>,
    ) -> Self {
        Self {
            actor_repo,
            auth_service,
        }
    }

    #[instrument(skip(self), fields(parent_id = %cmd.parent_actor_id, handle = %cmd.handle))]
    pub async fn execute(
        &self,
        cmd: CreateServiceAccountCommand,
    ) -> Result<CreateServiceAccountResult, DomainError> {
        // 1. Vérifier que le parent existe et est de type Human
        let parent = self
            .actor_repo
            .find_by_id(&cmd.parent_actor_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: cmd.parent_actor_id,
            })?;

        if !parent.is_human() {
            return Err(DomainError::BusinessRule(
                "Seul un acteur humain peut créer un Service Account. \
                 Les bots ne peuvent pas créer d'autres bots."
                    .to_string(),
            ));
        }

        // 2. Valider le handle (mêmes règles que register_actor)
        validate_bot_handle(&cmd.handle)?;

        // 3. Vérifier la liste noire de noms réservés
        if is_reserved_handle(&cmd.handle) {
            return Err(DomainError::BusinessRule(format!(
                "Le handle '{}' est réservé par le système SHINOBI et ne peut pas être utilisé",
                cmd.handle
            )));
        }

        // 4. Vérifier l'unicité du handle dans le namespace universel
        if self
            .actor_repo
            .find_by_handle(&cmd.handle)
            .await?
            .is_some()
        {
            return Err(DomainError::Duplicate(format!(
                "Handle '{}' déjà pris",
                cmd.handle
            )));
        }

        // 5. Créer l'Actor (AI Agent avec parent_id)
        let bot = Actor::new_service_account(
            &cmd.handle,
            &cmd.display_name,
            cmd.parent_actor_id,
        );
        self.actor_repo.save(&bot).await?;

        // 6. Générer un PAT
        let (raw_token, hash) = self.auth_service.generate_pat();

        // 7. Stocker le hash dans credentials (type api_key)
        let pat_label = cmd
            .label
            .unwrap_or_else(|| format!("{}-initial-pat", cmd.handle));
        self.actor_repo
            .save_credential(&bot.id, "api_key", &hash, None, Some(&pat_label))
            .await?;

        info!(
            bot_id = %bot.id,
            bot_handle = %bot.handle,
            parent_id = %cmd.parent_actor_id,
            parent_handle = %parent.handle,
            "🤖 Service Account créé — L'Acte de Naissance est signé"
        );

        // 8. Retourner le bot + token en clair
        Ok(CreateServiceAccountResult {
            actor: bot,
            raw_token,
        })
    }

    /// Liste les Service Accounts d'un acteur humain.
    #[instrument(skip(self))]
    pub async fn list(
        &self,
        parent_id: &Uuid,
    ) -> Result<Vec<Actor>, DomainError> {
        self.actor_repo.list_service_accounts(parent_id).await
    }

    /// Supprime un Service Account.
    /// Vérifie que le bot appartient bien au parent demandeur.
    #[instrument(skip(self))]
    pub async fn delete(
        &self,
        parent_id: &Uuid,
        bot_id: &Uuid,
    ) -> Result<bool, DomainError> {
        // Vérifier que le bot existe et appartient au parent
        let bot = self
            .actor_repo
            .find_by_id(bot_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "ServiceAccount",
                id: *bot_id,
            })?;

        if bot.parent_id != Some(*parent_id) {
            return Err(DomainError::Forbidden(
                "Ce Service Account ne vous appartient pas".to_string(),
            ));
        }

        if !bot.is_ai() {
            return Err(DomainError::BusinessRule(
                "Seuls les Service Accounts (AI Agents) peuvent être supprimés via cette route"
                    .to_string(),
            ));
        }

        let deleted = self.actor_repo.delete_service_account(bot_id).await?;

        if deleted {
            info!(
                bot_id = %bot_id,
                bot_handle = %bot.handle,
                parent_id = %parent_id,
                "🗑️ Service Account supprimé — L'Acte de Naissance est annulé"
            );
        }

        Ok(deleted)
    }
}

// ── Validation ────────────────────────────────────────────────────────

fn validate_bot_handle(handle: &str) -> Result<(), DomainError> {
    if handle.is_empty() || handle.len() > 39 {
        return Err(DomainError::BusinessRule(
            "Le handle du bot doit contenir entre 1 et 39 caractères".to_string(),
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

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_bot_handle_valid() {
        assert!(validate_bot_handle("ymclash-oracle-bot").is_ok());
        assert!(validate_bot_handle("hermes_v2").is_ok());
        assert!(validate_bot_handle("openclaw").is_ok());
    }

    #[test]
    fn test_validate_bot_handle_invalid() {
        assert!(validate_bot_handle("").is_err()); // empty
        assert!(validate_bot_handle("-bad").is_err()); // starts with dash
        assert!(validate_bot_handle("bad-").is_err()); // ends with dash
        assert!(validate_bot_handle("BAD").is_err()); // uppercase
        assert!(validate_bot_handle("has space").is_err()); // space
    }

    #[test]
    fn test_reserved_handles_blocked() {
        assert!(is_reserved_handle("oracle"));
        assert!(is_reserved_handle("SYSTEM"));
        assert!(is_reserved_handle("Admin"));
        assert!(!is_reserved_handle("ymclash-oracle-bot"));
    }
}
