//! Use Case: GetOperationDiff — Retrouver les fichiers modifiés par une opération VCS.
//!
//! Compare le tree de l'opération avec son parent pour identifier
//! les changements apportés par le commit Jujutsu.
//!
//! ## Phase 8.1 — Opérations racine
//! Pour les opérations sans parent (root), tous les fichiers sont considérés
//! comme "ajoutés". On résout les fichiers via IPFS (Merkle DAG) si disponible,
//! sinon via le VCS.

use std::sync::Arc;

use tracing::{info, instrument};
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::VcsEngine;

use super::resolve_ipfs;

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
    content_store: Option<Arc<dyn ContentStore>>,
}

impl GetOperationDiffUseCase {
    pub fn new(
        repository: Arc<dyn OperationRepository>,
        vcs: Arc<dyn VcsEngine>,
        content_store: Option<Arc<dyn ContentStore>>,
    ) -> Self {
        Self { repository, vcs, content_store }
    }

    /// Exécute le diff : retrouve l'opération, identifie le parent,
    /// puis compare les deux trees via `diff_since`.
    ///
    /// ## Opérations racine (pas de parent)
    /// Tous les fichiers sont considérés comme "ajoutés".
    /// On les résout via IPFS (Merkle DAG / Legacy) si un CID existe,
    /// sinon on retourne un vecteur vide (opération sans fichiers).
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
        let content_id = ContentId::new(operation.content_id.clone().into_inner());
        let repo_id = operation.repository_id;

        let changed_files = if !operation.parent_ids.is_empty() {
            // ── Avec parent → diff VCS classique ─────────────────────
            let parent_op = self.repository.find_by_id(&operation.parent_ids[0]).await?;
            match parent_op {
                Some(parent) => {
                    let parent_cid = ContentId::new(parent.content_id.into_inner());
                    self.vcs.diff_since(&repo_id, &parent_cid).await?
                }
                // Parent non trouvé en DB → diff contre vide
                None => self.vcs.diff_since(&repo_id, &content_id).await.unwrap_or_default(),
            }
        } else {
            // ── Sans parent → opération racine ───────────────────────
            // Phase 12A-Fix : utiliser diff_since() (qui compare vs empty tree
            // pour les root commits) au lieu de resolve_root_files() qui
            // renvoie des chemins IPFS bruts incompatibles avec l'affichage
            // de diffs unifiés dans le frontend Makimono.
            //
            // Fallback IPFS si le VCS engine n'est pas disponible.
            match self.vcs.diff_since(&repo_id, &content_id).await {
                Ok(files) if !files.is_empty() => {
                    info!(
                        operation_id = %id,
                        file_count = files.len(),
                        "📂 Root operation — diff VCS résolu (vs empty tree)"
                    );
                    files
                }
                _ => {
                    // Fallback IPFS si VCS echoue (repo non initialisé, etc.)
                    info!(
                        operation_id = %id,
                        "📂 Root operation — fallback IPFS (VCS indisponible)"
                    );
                    self.resolve_root_files(&operation).await
                }
            }
        };

        Ok(DiffResult {
            changed_files,
            operation_id: operation.id,
            content_id: operation.content_id.into_inner(),
        })
    }

    /// Résout les fichiers d'une opération racine via IPFS.
    ///
    /// Pour un commit sans parent, tous les fichiers sont "ajoutés".
    /// On utilise `resolve_ipfs_files` pour obtenir la liste des chemins.
    async fn resolve_root_files(
        &self,
        operation: &domain::entities::operation::Operation,
    ) -> Vec<String> {
        // Pas de CID IPFS → pas de fichiers à résoudre
        let ipfs_cid = match &operation.ipfs_content_id {
            Some(cid) => cid,
            None => return Vec::new(),
        };

        // Pas de ContentStore → pas d'accès IPFS
        let store = match &self.content_store {
            Some(s) => s,
            None => return Vec::new(),
        };

        // Résoudre les fichiers (Merkle DAG ou Legacy)
        match resolve_ipfs::resolve_ipfs_files(store.as_ref(), ipfs_cid).await {
            Ok(files) => {
                let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
                info!(
                    operation_id = %operation.id,
                    file_count = paths.len(),
                    "📂 Root operation — fichiers résolus via IPFS"
                );
                paths
            }
            Err(e) => {
                tracing::warn!(
                    operation_id = %operation.id,
                    error = %e,
                    "⚠️ Root operation — impossible de résoudre les fichiers IPFS"
                );
                Vec::new()
            }
        }
    }
}

