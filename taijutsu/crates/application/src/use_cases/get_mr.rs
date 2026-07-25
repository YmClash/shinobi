//! Use Case: GetMr — Récupération d'une MR avec détection dynamique de conflits.
//!
//! Le champ `has_conflicts` est calculé à la volée via `VcsEngine::can_fast_forward()`
//! plutôt que stocké en DB — garantit la fraîcheur de l'information.

use std::sync::Arc;

use tracing::instrument;
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

        // Détection dynamique de conflits (seulement si la MR est ouverte)
        let has_conflicts = if mr.is_open() {
            match self
                .vcs
                .can_fast_forward(&mr.repository_id, &mr.source_branch, &mr.target_branch)
                .await
            {
                Ok(can_ff) => !can_ff,
                Err(_) => true, // En cas d'erreur VCS, considérer comme conflit
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
