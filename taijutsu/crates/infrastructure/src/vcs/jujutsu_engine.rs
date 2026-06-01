//! Adaptateur VCS — Anti-Corruption Layer pour Jujutsu (jj-lib).
//!
//! Ce module isole l'API de jj-lib derrière le contrat stable `VcsEngine`.
//! L'Anti-Corruption Layer absorbe les évolutions de l'API jj-lib
//! (breaking changes entre versions) sans impacter les use cases.
//!
//! ## Stratégie
//! - **Phase 1** (actuelle) : Stub fonctionnel retournant des CIDs simulés.
//! - **Phase 2** : Intégration réelle avec jj-lib 0.41 (workspace en mémoire).
//! - **Phase N** : L'interface `VcsEngine` reste stable, seul cet adaptateur change.

use async_trait::async_trait;
use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::vcs_engine::VcsEngine;

/// Adaptateur jj-lib avec Anti-Corruption Layer.
///
/// En phase 1, cet adaptateur simule les opérations VCS.
/// L'interface reste identique pour les use cases.
#[derive(Debug, Clone)]
pub struct JujutsuEngine {
    /// Répertoire racine du workspace VCS.
    workspace_root: String,
}

impl JujutsuEngine {
    /// Construit un nouvel adaptateur pour le workspace donné.
    pub fn new(workspace_root: impl Into<String>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }
}

#[async_trait]
impl VcsEngine for JujutsuEngine {
    #[instrument(skip(self))]
    async fn init_workspace(&self, path: &str) -> Result<(), DomainError> {
        // Phase 2: jj_lib::workspace::Workspace::init(...)
        info!(
            path,
            workspace_root = %self.workspace_root,
            "Initialisation workspace VCS (stub phase 1)"
        );
        Ok(())
    }

    #[instrument(skip(self))]
    async fn create_operation(
        &self,
        description: &str,
        parent_ids: &[String],
    ) -> Result<ContentId, DomainError> {
        // Phase 2: Créer un vrai commit via jj_lib::repo::MutableRepo
        // et retourner le ChangeId / CommitId réel.
        let simulated_cid = format!("bafk_{}", Uuid::new_v4().simple());

        info!(
            description,
            parent_count = parent_ids.len(),
            cid = %simulated_cid,
            "Opération VCS créée (stub phase 1)"
        );

        Ok(ContentId::new(simulated_cid))
    }

    #[instrument(skip(self))]
    async fn resolve_head(&self) -> Result<Option<ContentId>, DomainError> {
        // Phase 2: jj_lib::repo::ReadonlyRepo::view().heads()
        info!("Résolution HEAD (stub phase 1)");
        Ok(None)
    }

    #[instrument(skip(self))]
    async fn diff_since(&self, content_id: &ContentId) -> Result<Vec<String>, DomainError> {
        // Phase 2: Comparer les trees entre le CID donné et HEAD.
        info!(
            cid = %content_id,
            "Diff depuis CID (stub phase 1)"
        );
        Ok(Vec::new())
    }
}
