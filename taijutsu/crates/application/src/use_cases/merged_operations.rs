//! Persistance des commits fusionnés côté serveur — Phase 42-pre-Fix 📜
//!
//! Un merge de MR écrit directement dans le store VCS : il ne passe pas par
//! `git-receive-pack`, donc le Sync Hook post-push ne crée aucune
//! `Operation`. Conséquence : le commit fusionné (ex. le fix du Sensei Bot)
//! n'apparaît ni dans la page Commits ni dans « Dernière opération ».
//!
//! Ce helper comble le trou : il remonte depuis la nouvelle tête jusqu'au
//! premier commit déjà connu en base, puis persiste les commits manquants
//! (ancêtres d'abord, pour que la lignée parentale se résolve).
//! **Non fatal** : toute erreur est loggée, le merge reste un succès.

use std::collections::HashSet;

use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::{CommitMeta, VcsEngine};

/// Garde-fou : nombre maximal de commits remontés par merge.
pub const MAX_MERGED_COMMITS: usize = 100;

/// Persiste les `Operation` des commits introduits par un merge.
///
/// Retourne le nombre d'opérations créées (0 en cas d'erreur).
pub async fn record_merged_operations(
    vcs: &dyn VcsEngine,
    op_repo: &dyn OperationRepository,
    repo_id: Uuid,
    author_id: Uuid,
    head_commit: &str,
) -> usize {
    match record_inner(vcs, op_repo, repo_id, author_id, head_commit).await {
        Ok(count) => {
            if count > 0 {
                info!(
                    repo_id = %repo_id,
                    head = %head_commit,
                    count,
                    "📜 Commits fusionnés persistés comme Operations"
                );
            }
            count
        }
        Err(e) => {
            warn!(
                repo_id = %repo_id,
                head = %head_commit,
                error = %e,
                "⚠️ Persistance des commits fusionnés échouée (non-fatal)"
            );
            0
        }
    }
}

async fn record_inner(
    vcs: &dyn VcsEngine,
    op_repo: &dyn OperationRepository,
    repo_id: Uuid,
    author_id: Uuid,
    head_commit: &str,
) -> Result<usize, DomainError> {
    // 1. Remonter le graphe jusqu'aux commits déjà connus en base.
    let mut missing: Vec<(String, CommitMeta)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![head_commit.to_string()];

    while let Some(sha) = stack.pop() {
        if !seen.insert(sha.clone()) {
            continue;
        }
        if missing.len() >= MAX_MERGED_COMMITS {
            warn!(repo_id = %repo_id, "📜 Limite de remontée atteinte — historique tronqué");
            break;
        }
        if op_repo.find_by_content_id(&repo_id, &sha).await?.is_some() {
            continue;
        }
        let Some(meta) = vcs.commit_meta(&repo_id, &sha).await? else {
            continue; // moteur sans support (mocks)
        };
        stack.extend(meta.parent_commit_ids.iter().cloned());
        missing.push((sha, meta));
    }

    // 2. Persister dans l'ordre topologique (parents manquants d'abord).
    let missing_set: HashSet<String> = missing.iter().map(|(s, _)| s.clone()).collect();
    let mut done: HashSet<String> = HashSet::new();
    let mut created = 0;

    while done.len() < missing.len() {
        let mut progressed = false;
        for (sha, meta) in &missing {
            if done.contains(sha) {
                continue;
            }
            let blocked = meta
                .parent_commit_ids
                .iter()
                .any(|p| missing_set.contains(p) && !done.contains(p));
            if blocked {
                continue;
            }

            let mut parent_ops = Vec::new();
            for p in &meta.parent_commit_ids {
                if let Some(op) = op_repo.find_by_content_id(&repo_id, p).await? {
                    parent_ops.push(op.id);
                }
            }

            let description = match meta.description.trim() {
                "" => "Merged commit".to_string(),
                d => d.to_string(),
            };
            let operation = Operation::new(
                author_id,
                repo_id,
                ContentId::new(sha.clone()),
                None,
                description,
                parent_ops,
            );
            match op_repo.save(&operation).await {
                Ok(()) => created += 1,
                Err(DomainError::Duplicate(_)) => {} // déjà persisté entre-temps
                Err(e) => return Err(e),
            }
            done.insert(sha.clone());
            progressed = true;
        }
        if !progressed {
            break; // cycle impossible en Git — garde-fou
        }
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::ports::vcs_engine::{FileDiff, RefInfo, TreeEntry};

    /// Mock VCS : graphe de commits en mémoire (`None` = non supporté).
    struct GraphVcs {
        commits: Option<HashMap<String, CommitMeta>>,
    }

    fn meta(desc: &str, parents: &[&str]) -> CommitMeta {
        CommitMeta {
            description: desc.to_string(),
            parent_commit_ids: parents.iter().map(|p| p.to_string()).collect(),
        }
    }

    #[async_trait]
    impl VcsEngine for GraphVcs {
        async fn commit_meta(&self, _: &Uuid, sha: &str) -> Result<Option<CommitMeta>, DomainError> {
            Ok(self.commits.as_ref().and_then(|c| c.get(sha).cloned()))
        }
        async fn init_workspace(&self, _: &Uuid, _: &Uuid) -> Result<(), DomainError> { Ok(()) }
        async fn create_operation(&self, _: &Uuid, _: &str, _: &[String], _: &[(String, Vec<u8>)]) -> Result<ContentId, DomainError> { Ok(ContentId::new("mock")) }
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

    /// Mock OperationRepository en mémoire.
    #[derive(Default)]
    struct MemOps {
        ops: Mutex<Vec<Operation>>,
    }

    impl MemOps {
        fn seeded(repo_id: Uuid, sha: &str) -> Self {
            let s = Self::default();
            s.ops.lock().unwrap().push(Operation::new(
                Uuid::new_v4(), repo_id, ContentId::new(sha.to_string()), None, "base", vec![],
            ));
            s
        }
        fn by_sha(&self, sha: &str) -> Option<Operation> {
            self.ops.lock().unwrap().iter().find(|o| o.content_id.as_str() == sha).cloned()
        }
    }

    #[async_trait]
    impl OperationRepository for MemOps {
        async fn save(&self, op: &Operation) -> Result<(), DomainError> {
            self.ops.lock().unwrap().push(op.clone());
            Ok(())
        }
        async fn find_by_id(&self, id: &Uuid) -> Result<Option<Operation>, DomainError> {
            Ok(self.ops.lock().unwrap().iter().find(|o| o.id == *id).cloned())
        }
        async fn find_by_content_id(&self, repo_id: &Uuid, sha: &str) -> Result<Option<Operation>, DomainError> {
            Ok(self
                .ops
                .lock()
                .unwrap()
                .iter()
                .find(|o| o.repository_id == *repo_id && o.content_id.as_str() == sha)
                .cloned())
        }
        async fn list_recent(&self, _: &Uuid, _: usize) -> Result<Vec<Operation>, DomainError> { Ok(vec![]) }
        async fn find_by_author(&self, _: &Uuid) -> Result<Vec<Operation>, DomainError> { Ok(vec![]) }
        async fn count_by_repo(&self, _: &Uuid) -> Result<i64, DomainError> { Ok(0) }
    }

    #[tokio::test]
    async fn merged_commits_are_persisted_with_lineage() {
        let repo = Uuid::new_v4();
        let actor = Uuid::new_v4();
        // main = a (connu) ; la MR apporte b puis c (fix Sensei).
        let vcs = GraphVcs {
            commits: Some(HashMap::from([
                ("c".to_string(), meta("fix(Futon_Check): auto-heal", &["b"])),
                ("b".to_string(), meta("wip", &["a"])),
                ("a".to_string(), meta("base", &[])),
            ])),
        };
        let ops = MemOps::seeded(repo, "a");

        let n = record_merged_operations(&vcs, &ops, repo, actor, "c").await;

        assert_eq!(n, 2);
        let a = ops.by_sha("a").unwrap();
        let b = ops.by_sha("b").unwrap();
        let c = ops.by_sha("c").unwrap();
        assert_eq!(b.parent_ids, vec![a.id]);
        assert_eq!(c.parent_ids, vec![b.id]);
        assert_eq!(c.description, "fix(Futon_Check): auto-heal");
        assert_eq!(c.author_id, actor);
    }

    #[tokio::test]
    async fn already_known_head_creates_nothing() {
        let repo = Uuid::new_v4();
        let vcs = GraphVcs { commits: Some(HashMap::from([("a".to_string(), meta("base", &[]))])) };
        let ops = MemOps::seeded(repo, "a");
        assert_eq!(record_merged_operations(&vcs, &ops, repo, Uuid::new_v4(), "a").await, 0);
        assert_eq!(ops.ops.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unsupported_engine_is_a_noop() {
        let repo = Uuid::new_v4();
        let vcs = GraphVcs { commits: None };
        let ops = MemOps::default();
        assert_eq!(record_merged_operations(&vcs, &ops, repo, Uuid::new_v4(), "c").await, 0);
        assert!(ops.ops.lock().unwrap().is_empty());
    }
}
