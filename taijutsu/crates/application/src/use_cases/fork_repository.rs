//! Use Case: ForkRepository — Fork intra-instance d'un dépôt (Phase 37B).
//!
//! Orchestre le fork complet d'un dépôt existant :
//! 1. Résolution du dépôt source via owner/name
//! 2. Vérification d'accès (public ou collaborateur)
//! 3. Garde anti-doublon (un owner ne peut forker qu'une fois le même repo)
//! 4. Construction de l'entité Repository avec forked_from_id
//! 5. Persistence dans PostgreSQL
//! 6. Clone du workspace VCS (copie physique dans spawn_blocking)
//! 7. Ajout du propriétaire comme collaborateur Owner
//!
//! ## Erreurs
//! - `NotFound` si le dépôt source ou l'acteur n'existe pas
//! - `Duplicate` si l'acteur possède déjà un fork ou un repo du même nom
//! - `BusinessRule` si l'acteur tente de forker son propre repo
//! - `Unauthorized` si le repo est privé et l'acteur n'a pas accès

use std::sync::Arc;

use chrono::Utc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use domain::entities::repository::{Repository, Visibility};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::vcs_engine::VcsEngine;

// ── Use Case ──────────────────────────────────────────────────────────

/// Use case de fork intra-instance d'un dépôt.
pub struct ForkRepositoryUseCase {
    actor_repo: Arc<dyn ActorRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    vcs: Arc<dyn VcsEngine>,
}

impl ForkRepositoryUseCase {
    /// Construit le use case avec les adaptateurs injectés.
    pub fn new(
        actor_repo: Arc<dyn ActorRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        vcs: Arc<dyn VcsEngine>,
    ) -> Self {
        Self {
            actor_repo,
            repo_repo,
            vcs,
        }
    }

    /// Exécute le fork complet d'un dépôt.
    ///
    /// ## Paramètres
    /// - `source_owner`: handle du propriétaire du dépôt source
    /// - `source_repo`: nom (slug) du dépôt source
    /// - `forker_id`: UUID de l'acteur qui veut forker (extrait du JWT)
    ///
    /// ## Flow
    /// 1. Résoudre l'acteur source et le repo source
    /// 2. Vérifier que l'acteur ne forke pas son propre repo
    /// 3. Vérifier l'accès (public ou collaborateur)
    /// 4. Vérifier qu'il n'a pas déjà un fork du même repo
    /// 5. Vérifier qu'il n'a pas déjà un repo du même nom
    /// 6. Créer l'entité Repository avec forked_from_id
    /// 7. Persister en base
    /// 8. Cloner le workspace VCS
    /// 9. Ajouter l'acteur comme collaborateur Owner
    ///
    /// ## GitHub-style naming
    /// Le fork prend le même slug que le repo parent.
    /// Si l'acteur possède déjà un repo du même nom → 409 Conflict.
    #[instrument(skip(self), fields(source = %source_owner, repo = %source_repo, forker = %forker_id))]
    pub async fn execute(
        &self,
        source_owner: &str,
        source_repo: &str,
        forker_id: &Uuid,
    ) -> Result<Repository, DomainError> {
        // ── Étape 1 : Résoudre l'acteur source ──────────────────────
        let source_actor = self
            .actor_repo
            .find_by_handle(source_owner)
            .await?
            .ok_or_else(|| DomainError::BusinessRule(
                format!("Acteur '{}' introuvable", source_owner),
            ))?;

        // ── Étape 1b : Résoudre le repo source ─────────────────────
        let source = self
            .repo_repo
            .find_by_owner_and_name(&source_actor.id, source_repo)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: Uuid::nil(),
            })?;

        // ── Étape 2 : Interdire le fork de son propre repo ──────────
        if source.owner_id == *forker_id {
            return Err(DomainError::BusinessRule(
                "Impossible de forker votre propre dépôt".into(),
            ));
        }

        // ── Étape 3 : Vérifier l'accès (public ou collaborateur) ────
        if !source.is_public() {
            let is_collab = self
                .repo_repo
                .is_collaborator(forker_id, &source.id)
                .await
                .unwrap_or(false);
            if !is_collab {
                // Ne pas révéler l'existence du repo privé → 404
                return Err(DomainError::NotFound {
                    entity_type: "Repository",
                    id: Uuid::nil(),
                });
            }
        }

        // ── Étape 4 : Garde anti-doublon (un fork par owner) ────────
        if let Some(existing_fork) = self
            .repo_repo
            .find_fork_by_owner(forker_id, &source.id)
            .await?
        {
            return Err(DomainError::Duplicate(format!(
                "Vous possédez déjà un fork de ce dépôt : '{}'",
                existing_fork.name,
            )));
        }

        // ── Étape 5 : Vérifier qu'aucun repo du même nom n'existe ──
        if self
            .repo_repo
            .find_by_owner_and_name(forker_id, &source.name)
            .await?
            .is_some()
        {
            return Err(DomainError::Duplicate(format!(
                "Vous possédez déjà un dépôt portant ce nom : '{}'",
                source.name,
            )));
        }

        // ── Étape 6 : Construire l'entité fork ─────────────────────
        let fork_id = Uuid::new_v4();
        let fork = Repository {
            id: fork_id,
            owner_id: *forker_id,
            name: source.name.clone(),
            display_name: source.name.clone(), // Utilise le slug, pas le display_name du parent
            description: source.description.clone(),
            visibility: Visibility::Public, // Les forks sont publics par défaut
            default_branch: source.default_branch.clone(),
            created_at: Utc::now(),
            mirror_source_url: None,
            mirror_synced_at: None,
            deleted_at: None,
            forked_from_id: Some(source.id),
        };

        // ── Étape 7 : Persister en base ─────────────────────────────
        self.repo_repo.save(&fork).await?;

        info!(
            fork_id = %fork_id,
            source_id = %source.id,
            forker = %forker_id,
            "🍴 Fork créé en base (PG)"
        );

        // ── Étape 8 : Cloner le workspace VCS ──────────────────────
        if let Err(e) = self
            .vcs
            .clone_workspace(&source.owner_id, &source.id, forker_id, &fork_id)
            .await
        {
            // Rollback : supprimer l'entrée PG si le clone échoue
            warn!(
                fork_id = %fork_id,
                error = %e,
                "⚠️ Clone VCS échoué — rollback PG"
            );
            let _ = self.repo_repo.hard_delete(&fork_id).await;
            return Err(e);
        }

        info!(
            fork_id = %fork_id,
            "🍴 Workspace VCS cloné avec succès"
        );

        // ── Étape 9 : Ajouter le forker comme collaborateur Owner ───
        self.repo_repo
            .add_collaborator(forker_id, &fork_id, "owner")
            .await?;

        info!(
            fork_id = %fork_id,
            source = %format!("{}/{}", source_owner, source_repo),
            "✅ Fork complet — Le Dédoublement est accompli"
        );

        Ok(fork)
    }
}
