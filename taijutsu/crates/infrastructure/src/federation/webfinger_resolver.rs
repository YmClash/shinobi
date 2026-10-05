//! WebFinger Resolver — Phase 37F (Le Mégaphone Interstellaire).
//!
//! Résout une mention fédérée `@handle@domain` en inbox ActivityPub :
//! 1. WebFinger lookup → actor URI (`rel=self`, `application/activity+json`)
//! 2. AP actor fetch (via `RemoteActorFetcher`) → inbox URL
//!
//! ## Sécurité
//! - Hérite du SSRF guard de `RemoteActorFetcher` (pas de localhost/private IP)
//! - Timeout strict sur le WebFinger (5s)
//! - Cache transitif via le fetcher (DashMap TTL 5 min)
//!
//! ## Pourquoi pas réutiliser notre propre handler WebFinger ?
//! Notre handler sert les requêtes *entrantes* (d'autres instances nous interrogent).
//! Ici on fait des requêtes *sortantes* vers d'autres instances (Mastodon, Forgejo, etc.).

use tracing::{info, warn};

use super::remote_actor::{RemoteActorFetcher, RemoteActorProfile};

/// Acteur distant résolu — prêt pour la livraison.
#[derive(Debug, Clone)]
pub struct ResolvedRemoteActor {
    /// URI ActivityPub de l'acteur (ex: `https://mastodon.social/users/alice`).
    pub actor_uri: String,
    /// URL de l'inbox (ex: `https://mastodon.social/users/alice/inbox`).
    pub inbox_url: String,
    /// Handle complet (ex: `alice@mastodon.social`).
    pub full_handle: String,
}

/// Erreurs de résolution WebFinger.
#[derive(Debug, thiserror::Error)]
pub enum WebFingerError {
    #[error("WebFinger lookup failed for {0}: {1}")]
    LookupFailed(String, String),

    #[error("No ActivityPub self link in WebFinger response for {0}")]
    NoSelfLink(String),

    #[error("Actor fetch failed for {0}: {1}")]
    ActorFetchFailed(String, String),
}

/// Résout une mention distante `@handle@domain` en inbox AP via WebFinger.
///
/// ## Flow
/// 1. `GET https://{domain}/.well-known/webfinger?resource=acct:{handle}@{domain}`
/// 2. Extraire le lien `rel=self` avec `type=application/activity+json` → actor URI
/// 3. Fetch le profil AP (via `RemoteActorFetcher`) → inbox URL
///
/// ## Exemples
/// ```text
/// resolve_mention("alice", "mastodon.social", &fetcher)
///   → ResolvedRemoteActor {
///       actor_uri: "https://mastodon.social/users/alice",
///       inbox_url: "https://mastodon.social/users/alice/inbox",
///       full_handle: "alice@mastodon.social",
///     }
/// ```
pub async fn resolve_mention(
    handle: &str,
    domain: &str,
    fetcher: &RemoteActorFetcher,
) -> Result<ResolvedRemoteActor, WebFingerError> {
    let full_handle = format!("{}@{}", handle, domain);
    let resource = format!("acct:{}", full_handle);
    let webfinger_url = format!(
        "https://{}/.well-known/webfinger?resource={}",
        domain, resource
    );

    info!(
        handle = %full_handle,
        url = %webfinger_url,
        "📡 WebFinger lookup — résolution mention fédérée"
    );

    // ── 1. WebFinger HTTP GET ──
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent("SHINOBI/0.1.0 (+https://jjshinobi.dev)")
        .build()
        .map_err(|e| WebFingerError::LookupFailed(full_handle.clone(), e.to_string()))?;

    let response = client
        .get(&webfinger_url)
        .header("Accept", "application/jrd+json, application/json")
        .send()
        .await
        .map_err(|e| {
            warn!(handle = %full_handle, error = %e, "❌ WebFinger request failed");
            WebFingerError::LookupFailed(full_handle.clone(), e.to_string())
        })?;

    if !response.status().is_success() {
        let status = response.status();
        warn!(handle = %full_handle, status = %status, "❌ WebFinger returned non-success");
        return Err(WebFingerError::LookupFailed(
            full_handle,
            format!("HTTP {}", status),
        ));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| {
        WebFingerError::LookupFailed(full_handle.clone(), format!("JSON parse error: {}", e))
    })?;

    // ── 2. Extraire le lien self (application/activity+json) ──
    let actor_uri = extract_self_link(&body).ok_or_else(|| {
        warn!(
            handle = %full_handle,
            "❌ No rel=self link with application/activity+json in WebFinger response"
        );
        WebFingerError::NoSelfLink(full_handle.clone())
    })?;

    info!(
        handle = %full_handle,
        actor_uri = %actor_uri,
        "📡 WebFinger → actor URI résolu"
    );

    // ── 3. Fetch le profil AP → inbox URL ──
    let profile: RemoteActorProfile = fetcher.fetch(&actor_uri).await.map_err(|e| {
        warn!(
            handle = %full_handle,
            actor_uri = %actor_uri,
            error = %e,
            "❌ AP actor fetch failed after WebFinger"
        );
        WebFingerError::ActorFetchFailed(full_handle.clone(), e.to_string())
    })?;

    let inbox_url = if profile.inbox.is_empty() {
        // Fallback standard AP
        format!("{}/inbox", actor_uri)
    } else {
        profile.inbox
    };

    info!(
        handle = %full_handle,
        inbox = %inbox_url,
        "📡✅ Mention fédérée résolue — inbox trouvé"
    );

    Ok(ResolvedRemoteActor {
        actor_uri,
        inbox_url,
        full_handle,
    })
}

/// Extrait le lien `rel=self` avec `type=application/activity+json` du JRD.
///
/// Format WebFinger (RFC 7033) :
/// ```json
/// {
///   "links": [
///     { "rel": "self", "type": "application/activity+json", "href": "https://..." }
///   ]
/// }
/// ```
fn extract_self_link(jrd: &serde_json::Value) -> Option<String> {
    let links = jrd.get("links")?.as_array()?;

    for link in links {
        let rel = link.get("rel")?.as_str()?;
        let link_type = link.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let href = link.get("href")?.as_str()?;

        if rel == "self"
            && (link_type == "application/activity+json"
                || link_type == "application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"")
        {
            return Some(href.to_string());
        }
    }

    None
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_self_link_activity_json() {
        let jrd = serde_json::json!({
            "subject": "acct:alice@mastodon.social",
            "links": [
                {
                    "rel": "http://webfinger.net/rel/profile-page",
                    "type": "text/html",
                    "href": "https://mastodon.social/@alice"
                },
                {
                    "rel": "self",
                    "type": "application/activity+json",
                    "href": "https://mastodon.social/users/alice"
                }
            ]
        });

        let result = extract_self_link(&jrd);
        assert_eq!(result, Some("https://mastodon.social/users/alice".to_string()));
    }

    #[test]
    fn test_extract_self_link_ld_json() {
        let jrd = serde_json::json!({
            "links": [
                {
                    "rel": "self",
                    "type": "application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
                    "href": "https://forgejo.example.com/api/v1/activitypub/user-id/1"
                }
            ]
        });

        let result = extract_self_link(&jrd);
        assert_eq!(
            result,
            Some("https://forgejo.example.com/api/v1/activitypub/user-id/1".to_string())
        );
    }

    #[test]
    fn test_extract_self_link_missing() {
        let jrd = serde_json::json!({
            "links": [
                { "rel": "http://webfinger.net/rel/profile-page", "type": "text/html", "href": "https://example.com" }
            ]
        });

        assert!(extract_self_link(&jrd).is_none());
    }

    #[test]
    fn test_extract_self_link_empty_links() {
        let jrd = serde_json::json!({ "links": [] });
        assert!(extract_self_link(&jrd).is_none());
    }

    #[test]
    fn test_extract_self_link_no_links() {
        let jrd = serde_json::json!({ "subject": "acct:test@example.com" });
        assert!(extract_self_link(&jrd).is_none());
    }
}
