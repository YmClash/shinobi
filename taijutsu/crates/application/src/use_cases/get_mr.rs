//! Use Case: GetMr — Récupération d'une MR avec détection dynamique de conflits.
//!
//! Le champ `has_conflicts` est calculé à la volée via `VcsEngine::can_fast_forward()`
//! plutôt que stocké en DB — garantit la fraîcheur de l'information.
//!
//! ## Phase 37E-Fix2 — Cross-Repo
//!
//! Pour les MR cross-repo (fork → parent), on doit :
//! 1. Fetch les refs du fork dans le parent (`fetch_fork_refs`)
//! 2. Utiliser la ref effective `fork-{id}/{branch}` au lieu de `source_branch`
//!
//! **Pas de cleanup ici** — les actions de lecture (Get, Diff) ne doivent jamais
//! détruire les refs du remote temporaire, car elles sont exécutées en parallèle
//! par le frontend. Le cleanup est réservé aux actions terminales (Merge, Close).

use std::sync::Arc;

use tracing::{instrument, warn};
use uuid::Uuid;

use domain::entities::merge_request::{MergeRequest, MrEvent, MrReview};
use domain::errors::DomainError;
use domain::ports::mr_repository::MrRepository;
use domain::ports::vcs_engine::VcsEngine;

/// Résultat enrichi d'un GET MR.
#[derive(Debug)]
pub struct MrDetail {
    /// La MR elle-même.
    pub mr: MergeRequest,
    /// Reviews associées.
    pub reviews: Vec<MrReview>,
    /// Timeline d'événements.
    pub events: Vec<MrEvent>,
    /// Conflit détecté dynamiquement (branches divergées).
    pub has_conflicts: bool,
}

pub struct GetMrUseCase {
    mr_repo: Arc<dyn MrRepository>,
    vcs: Arc<dyn VcsEngine>,
}

impl GetMrUseCase {
    pub fn new(mr_repo: Arc<dyn MrRepository>, vcs: Arc<dyn VcsEngine>) -> Self {
        Self { mr_repo, vcs }
    }

    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<MrDetail, DomainError> {
        let mr = self
            .mr_repo
            .find_by_repo_and_number(repo_id, number)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "MergeRequest",
                id: *repo_id,
            })?;

        let reviews = self.mr_repo.list_reviews(&mr.id).await?;
        let events = self.mr_repo.list_events(&mr.id).await?;

        // ── Phase 37E-Fix2 — Détection dynamique de conflits ─────────────
        //
        // Pour les MR cross-repo, on doit d'abord importer les refs du fork
        // dans le workspace du parent, puis utiliser la ref effective
        // (refs/remotes/fork-{id}/{branch}) pour la comparaison.
        //
        // Pas de cleanup ici : les requêtes GET /mrs/{n} et GET /mrs/{n}/diff
        // sont parallélisées par le frontend Next.js. Détruire le remote
        // couperait l'herbe sous le pied de MrDiffUseCase.
        let has_conflicts = if mr.is_open() {
            // Construire la ref source effective
            let effective_source = if let Some(src_id) = mr.source_repository_id {
                // Cross-repo : fetch des objets Git du fork → refs/remotes/fork-{id}/*
                if let Err(e) = self.vcs.fetch_fork_refs(&mr.repository_id, &src_id).await {
                    warn!(
                        mr_id = %mr.id,
                        source_repo = %src_id,
                        error = %e,
                        "🕳️ fetch_fork_refs échoué pour conflict check — on assume pas de conflit"
                    );
                    // Si le fetch échoue, on ne peut pas vérifier → pas de faux positif
                    return Ok(MrDetail { mr, reviews, events, has_conflicts: false });
                }
                // La branche source est dans le remote temporaire
                format!("fork-{}/{}", src_id, mr.source_branch)
            } else {
                // Intra-repo : la branche source est locale
                mr.source_branch.clone()
            };

            match self
                .vcs
                .can_fast_forward(&mr.repository_id, &effective_source, &mr.target_branch)
                .await
            {
                Ok(can_ff) => !can_ff,
                Err(e) => {
                    // Phase 37E-Fix2 : un faux positif de conflit bloque le travail,
                    // un faux négatif transitoire sera rattrapé par le vrai merge.
                    warn!(
                        mr_id = %mr.id,
                        source = %effective_source,
                        target = %mr.target_branch,
                        error = %e,
                        "can_fast_forward échoué — on assume pas de conflit"
                    );
                    false
                }
            }
        } else {
            false
        };

        Ok(MrDetail {
            mr,
            reviews,
            events,
            has_conflicts,
        })
    }
}

