//! Nettoyage des branches shadow du Kage Bunshin — Phase 42-pre-Fix 🧹
//!
//! Après le merge ou la fermeture d'une MR auto-heal, la branche
//! `kage-bunshin/*` n'a plus d'utilité. Ce helper la supprime de façon
//! **non fatale** : une erreur est loggée mais ne fait jamais échouer
//! le merge / la fermeture. Les branches humaines ne sont jamais touchées.

use tracing::{info, warn};

use domain::entities::merge_request::MergeRequest;
use domain::ports::vcs_engine::VcsEngine;

/// Préfixe réservé aux branches shadow créées par le Kage Bunshin.
pub const SHADOW_BRANCH_PREFIX: &str = "kage-bunshin/";

/// Supprime la branche source de la MR si (et seulement si) c'est une
/// branche shadow d'une MR intra-repo.
pub async fn cleanup_shadow_branch(vcs: &dyn VcsEngine, mr: &MergeRequest) {
    if mr.is_cross_repo() || !mr.source_branch.starts_with(SHADOW_BRANCH_PREFIX) {
        return;
    }

    match vcs.delete_branch(&mr.repository_id, &mr.source_branch).await {
        Ok(true) => info!(
            mr_id = %mr.id,
            branch = %mr.source_branch,
            "🧹 Branche shadow supprimée"
        ),
        Ok(false) => info!(
            mr_id = %mr.id,
            branch = %mr.source_branch,
            "🧹 Branche shadow absente — rien à supprimer"
        ),
        Err(e) => warn!(
            mr_id = %mr.id,
            branch = %mr.source_branch,
            error = %e,
            "⚠️ Suppression de la branche shadow échouée (non-fatal)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::entities::content_id::ContentId;
    use domain::errors::DomainError;
    use domain::ports::vcs_engine::{FileDiff, RefInfo, TreeEntry};
    use uuid::Uuid;

    /// Mock VCS : enregistre les `delete_branch`, peut simuler un échec.
    /// Les autres méthodes ne sont jamais appelées par le helper.
    struct RecordingVcs {
        deleted: Mutex<Vec<String>>,
        fail: bool,
    }

    #[async_trait]
    impl VcsEngine for RecordingVcs {
        async fn delete_branch(&self, _: &Uuid, branch: &str) -> Result<bool, DomainError> {
            self.deleted.lock().unwrap().push(branch.to_string());
            if self.fail {
                Err(DomainError::VcsError("boom".into()))
            } else {
                Ok(true)
            }
        }
        async fn init_workspace(&self, _: &Uuid, _: &Uuid) -> Result<(), DomainError> { Ok(()) }
        async fn create_operation(&self, _: &Uuid, _: &str, _: &[String], _: &[(String, Vec<u8>)]) -> Result<ContentId, DomainError> { Ok(ContentId::new("mock".to_string())) }
        async fn resolve_head(&self, _: &Uuid) -> Result<Option<ContentId>, DomainError> { Ok(None) }
        async fn diff_since(&self, _: &Uuid, _: &ContentId) -> Result<Vec<String>, DomainError> { Ok(vec![]) }
        async fn list_tree(&self, _: &Uuid, _: &str, _: &str) -> Result<Vec<TreeEntry>, DomainError> { Ok(vec![]) }
        async fn read_blob(&self, _: &Uuid, _: &str, _: &str) -> Result<Vec<u8>, DomainError> { Ok(vec![]) }
        async fn list_refs(&self, _: &Uuid) -> Result<Vec<RefInfo>, DomainError> { Ok(vec![]) }
        async fn diff_content(&self, _: &Uuid, _: &ContentId) -> Result<Vec<FileDiff>, DomainError> { Ok(vec![]) }
        async fn can_fast_forward(&self, _: &Uuid, _: &str, _: &str) -> Result<bool, DomainError> { Ok(true) }
        async fn merge_fast_forward(&self, _: &Uuid, _: &str, _: &str) -> Result<ContentId, DomainError> { Ok(ContentId::new("m")) }
        async fn squash_merge(&self, _: &Uuid, _: &str, _: &str, _: &str) -> Result<ContentId, DomainError> { Ok(ContentId::new("m")) }
        async fn diff_merge_base(&self, _: &Uuid, _: &str, _: &str) -> Result<Vec<FileDiff>, DomainError> { Ok(vec![]) }
        async fn clone_workspace(&self, _: &Uuid, _: &Uuid, _: &Uuid, _: &Uuid) -> Result<(), DomainError> { Ok(()) }
        async fn fetch_fork_refs(&self, _: &Uuid, _: &Uuid) -> Result<(), DomainError> { Ok(()) }
        async fn cleanup_fork_remote(&self, _: &Uuid, _: &Uuid) -> Result<(), DomainError> { Ok(()) }
    }

    fn mr(source: &str) -> MergeRequest {
        MergeRequest::new(Uuid::new_v4(), Uuid::new_v4(), 1, "t".into(), None, source.into(), "main".into())
    }

    #[tokio::test]
    async fn shadow_branch_is_deleted() {
        let vcs = RecordingVcs { deleted: Mutex::new(vec![]), fail: false };
        cleanup_shadow_branch(&vcs, &mr("kage-bunshin/abc-Futon_Check")).await;
        assert_eq!(*vcs.deleted.lock().unwrap(), vec!["kage-bunshin/abc-Futon_Check"]);
    }

    #[tokio::test]
    async fn human_branch_is_never_touched() {
        let vcs = RecordingVcs { deleted: Mutex::new(vec![]), fail: false };
        cleanup_shadow_branch(&vcs, &mr("feature/auth")).await;
        assert!(vcs.deleted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cross_repo_mr_is_never_touched() {
        let vcs = RecordingVcs { deleted: Mutex::new(vec![]), fail: false };
        let mut m = mr("kage-bunshin/x");
        m.source_repository_id = Some(Uuid::new_v4());
        cleanup_shadow_branch(&vcs, &m).await;
        assert!(vcs.deleted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_failure_is_swallowed() {
        let vcs = RecordingVcs { deleted: Mutex::new(vec![]), fail: true };
        // Ne doit ni paniquer ni propager d'erreur (retour `()`).
        cleanup_shadow_branch(&vcs, &mr("kage-bunshin/x")).await;
        assert_eq!(vcs.deleted.lock().unwrap().len(), 1);
    }
}
