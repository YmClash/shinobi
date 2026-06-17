//! Use Case: GetOperationDiff — Retrouver les fichiers modifiés par une opération VCS.
//!
//! Compare le tree de l'opération avec son parent pour identifier
//! les changements apportés par le commit Jujutsu.

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::VcsEngine;

/// Résultat du diff d'une opération.
pub struct DiffResult {
    /// Chemins des fichiers modifiés (relatifs au workspace).
    pub changed_files: Vec<String>,
    /// Identifiant de l'opération.
    pub operation_id: Uuid,
    /// Content ID (commit jj) de l'opération.
    pub content_id: String,
}

/// Use case: récupérer les fichiers modifiés par une opération.
pub struct GetOperationDiffUseCase {
    repository: Arc<dyn OperationRepository>,
    vcs: Arc<dyn VcsEngine>,
}

impl GetOperationDiffUseCase {
    pub fn new(
        repository: Arc<dyn OperationRepository>,
        vcs: Arc<dyn VcsEngine>,
    ) -> Self {
        Self { repository, vcs }
    }

    /// Exécute le diff : retrouve l'opération, identifie le parent,
    /// puis compare les deux trees via `diff_since`.
    #[instrument(skip(self))]
    pub async fn execute(&self, id: Uuid) -> Result<DiffResult, DomainError> {
        // 1. Retrouver l'opération
        let operation = self
            .repository
            .find_by_id(&id)
            .await?
            .ok_or(DomainError::NotFound {
                entity_type: "Operation",
                id,
            })?;

        // 2. Déterminer le content_id de référence pour le diff
        // Si l'opération a des parents, on diff depuis le premier parent.
        // Sinon, on diff depuis l'opération elle-même (résultat = vide).
        let content_id = ContentId::new(operation.content_id.clone().into_inner());

        let changed_files = if !operation.parent_ids.is_empty() {
            // Retrouver le parent pour calculer le diff
            let parent_op = self.repository.find_by_id(&operation.parent_ids[0]).await?;
            match parent_op {
                Some(parent) => {
                    let parent_cid = ContentId::new(parent.content_id.into_inner());
                    self.vcs.diff_since(&parent_cid).await?
                }
                // Parent non trouvé en DB → diff contre vide
                None => self.vcs.diff_since(&content_id).await.unwrap_or_default(),
            }
        } else {
            // Pas de parent → première opération, tout est "ajouté"
            // diff_since sur le content_id de l'opération vs HEAD
            // montre les fichiers modifiés par ce commit
            Vec::new()
        };

        Ok(DiffResult {
            changed_files,
            operation_id: operation.id,
            content_id: operation.content_id.into_inner(),
        })
    }
}
