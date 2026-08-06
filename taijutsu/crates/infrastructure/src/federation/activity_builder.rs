//! Constructeurs d'activités ForgeFed — Phase 27-ter.
//!
//! Construit des activités JSON-LD conformes à la spécification ForgeFed
//! pour les événements forge (création de repo, push de commits).
//!
//! ## Références
//! - ForgeFed §3.1 — Creating Resource Actors
//! - ForgeFed §3.6.2 — Reporting Pushed Commits
//! - ActivityStreams 2.0 — Vocabulary

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use domain::entities::repository::Repository;

/// Nombre maximum de commits inclus dans une activité Push.
/// Au-delà, seul le HEAD est référencé (protection contre les payloads géants).
const MAX_PUSH_COMMITS: usize = 5;

/// Construit une activité `Create { Repository }` conforme ForgeFed.
///
/// Générée automatiquement lors de la création d'un dépôt.
/// L'objet est un `Repository` ForgeFed avec les métadonnées de base.
///
/// ## Exemple JSON-LD
/// ```json
/// {
///   "@context": ["https://www.w3.org/ns/activitystreams", "https://forgefed.org/ns"],
///   "type": "Create",
///   "actor": "https://domain/actors/owner",
///   "object": { "type": "Repository", ... }
/// }
/// ```
pub fn create_repository_activity(
    domain: &str,
    owner_handle: &str,
    repo: &Repository,
) -> serde_json::Value {
    let scheme = federation_scheme(domain);
    let actor_uri = format!("{}://{}/actors/{}", scheme, domain, owner_handle);
    let repo_uri = format!("{}://{}/repos/{}/{}", scheme, domain, owner_handle, repo.name);
    let activity_id = format!("{}://{}/activities/{}", scheme, domain, Uuid::new_v4());

    json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://forgefed.org/ns"
        ],
        "id": activity_id,
        "type": "Create",
        "actor": actor_uri,
        "published": Utc::now().to_rfc3339(),
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{}/followers", actor_uri)],
        "object": {
            "type": "Repository",
            "id": repo_uri,
            "name": repo.display_name,
            "summary": repo.description.clone().unwrap_or_default(),
            "attributedTo": actor_uri,
            "published": repo.created_at.to_rfc3339(),
        }
    })
}

/// Informations sur un commit pour l'activité Push.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// SHA-1 du commit (tronqué à 7 caractères dans l'affichage).
    pub sha: String,
    /// Message du commit (première ligne).
    pub message: String,
}

/// Construit une activité `Push` conforme ForgeFed §3.6.2.
///
/// Générée automatiquement après un `git push` réussi.
/// Inclut au maximum `MAX_PUSH_COMMITS` (5) commits pour éviter
/// les payloads géants (protection contre le push de 10k commits).
///
/// ## Structure ForgeFed
/// ```json
/// {
///   "type": "Push",
///   "actor": "...",
///   "target": "refs/heads/main",
///   "object": { "type": "OrderedCollection", "items": [...commits...] }
/// }
/// ```
pub fn push_activity(
    domain: &str,
    owner_handle: &str,
    repo: &Repository,
    branch: &str,
    head_sha: &str,
    commits: &[CommitInfo],
) -> serde_json::Value {
    let scheme = federation_scheme(domain);
    let actor_uri = format!("{}://{}/actors/{}", scheme, domain, owner_handle);
    let repo_uri = format!("{}://{}/repos/{}/{}", scheme, domain, owner_handle, repo.name);
    let activity_id = format!("{}://{}/activities/{}", scheme, domain, Uuid::new_v4());

    let total_commits = commits.len();
    // Limiter à MAX_PUSH_COMMITS pour éviter les payloads géants
    let truncated_commits: Vec<serde_json::Value> = commits
        .iter()
        .take(MAX_PUSH_COMMITS)
        .map(|c| {
            json!({
                "type": "Commit",
                "id": format!("{}/commits/{}", repo_uri, c.sha),
                "hash": c.sha,
                "summary": c.message,
                "attributedTo": actor_uri,
            })
        })
        .collect();

    json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://forgefed.org/ns"
        ],
        "id": activity_id,
        "type": "Push",
        "actor": actor_uri,
        "published": Utc::now().to_rfc3339(),
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [format!("{}/followers", actor_uri)],
        "target": format!("{}/branches/{}", repo_uri, branch),
        "hashBefore": "",
        "hashAfter": head_sha,
        "totalCommits": total_commits,
        "object": {
            "type": "OrderedCollection",
            "totalItems": total_commits,
            "items": truncated_commits,
        },
        "context": repo_uri,
    })
}

/// Retourne "http" si le domaine contient "localhost", "https" sinon.
///
/// Évite les problèmes de signature HTTP en dev local.
fn federation_scheme(domain: &str) -> &'static str {
    if domain.contains("localhost") || domain.contains("127.0.0.1") {
        "http"
    } else {
        "https"
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repo() -> Repository {
        let mut repo = Repository::new(Uuid::new_v4(), "shinobi", "Shinobi Forge");
        repo.description = Some("A federated code forge".to_string());
        repo
    }

    #[test]
    fn test_create_repository_activity_structure() {
        let repo = make_repo();
        let activity = create_repository_activity("forge.shinobi.dev", "ymclash", &repo);

        assert_eq!(activity["type"], "Create");
        assert_eq!(activity["actor"], "https://forge.shinobi.dev/actors/ymclash");
        assert_eq!(activity["object"]["type"], "Repository");
        assert_eq!(activity["object"]["name"], "Shinobi Forge");
        assert_eq!(activity["object"]["summary"], "A federated code forge");
        assert!(activity["id"].as_str().unwrap().starts_with("https://"));
        assert!(activity["published"].as_str().is_some());

        // Vérifier la présence du contexte ForgeFed
        let contexts = activity["@context"].as_array().unwrap();
        assert!(contexts.iter().any(|c| c == "https://forgefed.org/ns"));
    }

    #[test]
    fn test_create_repository_activity_localhost_http() {
        let repo = make_repo();
        let activity = create_repository_activity("localhost:3000", "alice", &repo);

        // En dev local, les URIs doivent utiliser http://
        assert!(activity["id"].as_str().unwrap().starts_with("http://localhost"));
        assert_eq!(activity["actor"], "http://localhost:3000/actors/alice");
    }

    #[test]
    fn test_push_activity_structure() {
        let repo = make_repo();
        let commits = vec![
            CommitInfo { sha: "abc1234".into(), message: "feat: add federation".into() },
            CommitInfo { sha: "def5678".into(), message: "fix: typo".into() },
        ];

        let activity = push_activity(
            "forge.shinobi.dev", "ymclash", &repo, "main", "abc1234", &commits,
        );

        assert_eq!(activity["type"], "Push");
        assert_eq!(activity["hashAfter"], "abc1234");
        assert_eq!(activity["totalCommits"], 2);

        let items = activity["object"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "Commit");
        assert_eq!(items[0]["hash"], "abc1234");
    }

    #[test]
    fn test_push_activity_truncates_at_5() {
        let repo = make_repo();
        let commits: Vec<CommitInfo> = (0..20)
            .map(|i| CommitInfo {
                sha: format!("sha{:04}", i),
                message: format!("commit {}", i),
            })
            .collect();

        let activity = push_activity(
            "forge.shinobi.dev", "ymclash", &repo, "main", "sha0019", &commits,
        );

        // totalCommits = 20 (le vrai nombre)
        assert_eq!(activity["totalCommits"], 20);

        // Mais items = 5 max (protection payload)
        let items = activity["object"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 5);

        // Le premier commit est sha0000
        assert_eq!(items[0]["hash"], "sha0000");
    }

    #[test]
    fn test_federation_scheme() {
        assert_eq!(federation_scheme("localhost:3000"), "http");
        assert_eq!(federation_scheme("127.0.0.1:3000"), "http");
        assert_eq!(federation_scheme("forge.shinobi.dev"), "https");
        assert_eq!(federation_scheme("shinobi.duckdns.org"), "https");
    }
}
