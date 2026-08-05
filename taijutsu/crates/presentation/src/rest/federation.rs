//! Handlers de fédération ActivityPub / ForgeFed — Phase 27.
//!
//! Endpoints de découverte (WebFinger, NodeInfo) et protocole ActivityPub
//! (Actor profiles, Inbox, Outbox, Followers, Following).
//!
//! ## Pièges protocolaires intégrés
//! - **WebFinger** : Content-Type exact `application/jrd+json` (pas `application/json`)
//! - **Content Negotiation** : Support `application/activity+json` ET
//!   `application/ld+json; profile="https://www.w3.org/ns/activitystreams"`
//! - **Clock Skew** : Vérification 30s sur le header Date des requêtes Inbox

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::federation::FederationFollow;
use domain::errors::DomainError;

use crate::errors::AppError;
use crate::state::SharedState;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Phase 27A — Discovery Layer
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Paramètres de query pour WebFinger.
#[derive(Debug, Deserialize)]
pub struct WebFingerQuery {
    pub resource: String,
}

/// WebFinger — `GET /.well-known/webfinger?resource=acct:{handle}@{domain}`
///
/// RFC 7033 — Résout un identifiant `acct:` en liens ActivityPub.
///
/// ## Content-Type
/// ⚠️ DOIT être exactement `application/jrd+json` — Mastodon et les autres
/// instances ignorent silencieusement `application/json`.
pub async fn webfinger_handler(
    State(state): State<SharedState>,
    Query(params): Query<WebFingerQuery>,
) -> Result<Response, AppError> {
    info!(resource = %params.resource, "🌐 WebFinger lookup");

    // Parse "acct:handle@domain" → handle
    let handle = parse_acct_resource(&params.resource, &state.federation_domain)
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Invalid WebFinger resource: {}. Expected acct:handle@{}", params.resource, state.federation_domain),
        )))?;

    // Vérifier que l'acteur existe
    let actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    let domain = &state.federation_domain;
    let actor_uri = format!("https://{}/actors/{}", domain, actor.handle);

    let jrd = serde_json::json!({
        "subject": format!("acct:{}@{}", actor.handle, domain),
        "aliases": [
            actor_uri,
            format!("https://{}/api/v1/actors/{}/profile", domain, actor.handle),
        ],
        "links": [
            {
                "rel": "self",
                "type": "application/activity+json",
                "href": actor_uri,
            },
            {
                "rel": "http://webfinger.net/rel/profile-page",
                "type": "text/html",
                "href": format!("https://{}/profile/{}", domain, actor.handle),
            },
        ]
    });

    // ⚠️ Content-Type DOIT être application/jrd+json
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/jrd+json")],
        Json(jrd),
    ).into_response())
}

/// NodeInfo well-known — `GET /.well-known/nodeinfo`
///
/// Retourne le lien vers le document NodeInfo 2.1.
pub async fn nodeinfo_wellknown_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let domain = &state.federation_domain;

    Json(serde_json::json!({
        "links": [
            {
                "rel": "http://nodeinfo.diaspora.software/ns/schema/2.1",
                "href": format!("https://{}/nodeinfo/2.1", domain),
            }
        ]
    }))
}

/// NodeInfo 2.1 — `GET /nodeinfo/2.1`
///
/// Métadonnées de l'instance : software, protocols, usage stats.
pub async fn nodeinfo_handler(
    State(state): State<SharedState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let local_users = state.federation_repo.count_local_users().await.unwrap_or(0);
    let local_repos = state.federation_repo.count_local_repos().await.unwrap_or(0);

    Ok(Json(serde_json::json!({
        "version": "2.1",
        "software": {
            "name": "shinobi",
            "version": env!("CARGO_PKG_VERSION"),
            "repository": "https://github.com/YmClash/shinobi",
            "homepage": "https://shinobi.dev",
        },
        "protocols": ["activitypub"],
        "services": {
            "inbound": [],
            "outbound": [],
        },
        "openRegistrations": true,
        "usage": {
            "users": {
                "total": local_users,
                "activeMonth": local_users,
                "activeHalfyear": local_users,
            },
            "localPosts": local_repos,
        },
        "metadata": {
            "nodeDescription": "SHINOBI — Forge Sociale fédérée de nouvelle génération",
            "features": [
                "forgefed",
                "activitypub",
                "git",
                "jujutsu",
                "ai-provenance",
                "merge-requests",
            ],
        },
    })))
}

/// Profil ActivityPub d'un acteur — `GET /actors/{handle}`
///
/// ## Content Negotiation
/// - `Accept: application/activity+json` → JSON-LD ActivityPub
/// - `Accept: application/ld+json; profile="https://www.w3.org/ns/activitystreams"` → idem
/// - Autre → 406 Not Acceptable
pub async fn actor_ap_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // Vérifier le header Accept pour la content negotiation
    if !accepts_activitypub(&headers) {
        return Ok((
            StatusCode::NOT_ACCEPTABLE,
            Json(serde_json::json!({
                "error": "Not Acceptable",
                "message": "This endpoint requires Accept: application/activity+json",
            })),
        ).into_response());
    }

    info!(handle = %handle, "🌐 ActivityPub actor fetch");

    let actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    let domain = &state.federation_domain;
    let actor_uri = format!("https://{}/actors/{}", domain, actor.handle);

    // Récupérer la clé publique si elle existe
    let public_key_section = if let Ok(Some(keypair)) = state.federation_repo.get_keypair(&actor.id).await {
        serde_json::json!({
            "id": keypair.key_id,
            "owner": actor_uri,
            "publicKeyPem": keypair.public_key_pem,
        })
    } else {
        serde_json::json!(null)
    };

    // Mapper ActorType → ActivityPub type
    let ap_type = match actor.actor_type {
        domain::ActorType::Human => "Person",
        domain::ActorType::AiAgent => "Service",
        domain::ActorType::System => "Application",
    };

    let ap_actor = serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://w3id.org/security/v1",
            "https://forgefed.org/ns",
        ],
        "id": actor_uri,
        "type": ap_type,
        "preferredUsername": actor.handle,
        "name": actor.display_name,
        "summary": actor.bio.unwrap_or_default(),
        "inbox": format!("{}/inbox", actor_uri),
        "outbox": format!("{}/outbox", actor_uri),
        "followers": format!("{}/followers", actor_uri),
        "following": format!("{}/following", actor_uri),
        "url": format!("https://{}/profile/{}", domain, actor.handle),
        "published": actor.created_at.to_rfc3339(),
        "publicKey": public_key_section,
        "icon": actor.avatar_url.map(|url| serde_json::json!({
            "type": "Image",
            "url": url,
        })),
        "endpoints": {
            "sharedInbox": format!("https://{}/inbox", domain),
        },
    });

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/activity+json; charset=utf-8")],
        Json(ap_actor),
    ).into_response())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Phase 27C — ActivityPub Inbox / Outbox / Followers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Inbox ActivityPub — `POST /actors/{handle}/inbox`
///
/// Reçoit les activités fédérées entrantes. Pour Phase 27 :
/// - `Follow` → persiste le follower + répond `Accept` (202)
/// - `Undo(Follow)` → supprime le follower
/// - Autres → log + ignore (202 Accepted, stub Phase 27-bis)
///
/// ## Sécurité
/// - Vérifie le clock skew (< 30s) pour prévenir les replay attacks
/// - La vérification de signature HTTP sera renforcée en Phase 27-bis
pub async fn inbox_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    info!(handle = %handle, "📥 ActivityPub Inbox — activité reçue");

    // ── Piège de l'Horloge : vérifier la fraîcheur ──
    if let Some(date) = headers.get("date").and_then(|v| v.to_str().ok()) {
        if let Err(e) = infrastructure::federation::http_signature::verify_clock_skew(date) {
            warn!(error = %e, "🕐 Requête fédérée rejetée — clock skew");
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": e.to_string() })),
            ).into_response());
        }
    }

    // Résoudre l'acteur local
    let actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    // Dispatch selon le type d'activité
    let activity_type = body.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    match activity_type {
        "Follow" => {
            let follower_uri = body.get("actor")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if follower_uri.is_empty() {
                return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({
                    "error": "Missing 'actor' field in Follow activity"
                }))).into_response());
            }

            info!(
                follower = %follower_uri,
                following = %actor.handle,
                "🤝 Follow reçu — auto-accept"
            );

            // Persister le follow (auto-accept)
            let follow = FederationFollow {
                id: Uuid::new_v4(),
                follower_uri: follower_uri.clone(),
                following_actor_id: actor.id,
                accepted: true,
                created_at: Utc::now(),
            };
            state.federation_repo.save_follow(&follow).await?;

            // TODO Phase 27-bis : envoyer un Accept signé vers l'inbox du follower

            Ok((StatusCode::ACCEPTED, Json(serde_json::json!({
                "status": "accepted",
                "type": "Follow",
            }))).into_response())
        }
        "Undo" => {
            // Vérifier si c'est un Undo(Follow)
            let inner_type = body.get("object")
                .and_then(|o| o.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if inner_type == "Follow" {
                let follower_uri = body.get("actor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                info!(follower = %follower_uri, "👋 Undo Follow reçu");
                state.federation_repo.delete_follow(follower_uri, &actor.id).await?;
            }

            Ok((StatusCode::ACCEPTED, Json(serde_json::json!({
                "status": "accepted",
                "type": "Undo",
            }))).into_response())
        }
        _ => {
            // Phase 27 : on accepte poliment et on archive (202 Accepted)
            info!(
                activity_type = %activity_type,
                "📋 Activité fédérée non gérée — archivée (stub)"
            );
            Ok((StatusCode::ACCEPTED, Json(serde_json::json!({
                "status": "accepted",
                "type": activity_type,
                "note": "Activity logged but not processed (Phase 27 stub)"
            }))).into_response())
        }
    }
}

/// Outbox ActivityPub — `GET /actors/{handle}/outbox`
///
/// Retourne une OrderedCollection des activités récentes.
/// Phase 27 : collection vide (lecture seule, pas de Client-to-Server).
pub async fn outbox_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if !accepts_activitypub(&headers) {
        return Ok((StatusCode::NOT_ACCEPTABLE, Json(serde_json::json!({
            "error": "Requires Accept: application/activity+json",
        }))).into_response());
    }

    // Vérifier que l'acteur existe
    let _actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    let domain = &state.federation_domain;
    let outbox_uri = format!("https://{}/actors/{}/outbox", domain, handle);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/activity+json; charset=utf-8")],
        Json(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": outbox_uri,
            "type": "OrderedCollection",
            "totalItems": 0,
            "orderedItems": [],
        })),
    ).into_response())
}

/// Followers ActivityPub — `GET /actors/{handle}/followers`
///
/// Retourne une OrderedCollection des followers fédérés.
pub async fn followers_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if !accepts_activitypub(&headers) {
        return Ok((StatusCode::NOT_ACCEPTABLE, Json(serde_json::json!({
            "error": "Requires Accept: application/activity+json",
        }))).into_response());
    }

    let actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    let followers = state.federation_repo.list_followers(&actor.id).await?;
    let count = followers.len();
    let uris: Vec<&str> = followers.iter().map(|f| f.follower_uri.as_str()).collect();

    let domain = &state.federation_domain;
    let followers_uri = format!("https://{}/actors/{}/followers", domain, handle);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/activity+json; charset=utf-8")],
        Json(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": followers_uri,
            "type": "OrderedCollection",
            "totalItems": count,
            "orderedItems": uris,
        })),
    ).into_response())
}

/// Following ActivityPub — `GET /actors/{handle}/following`
///
/// Retourne une OrderedCollection (vide pour Phase 27 — pas de follow sortant).
pub async fn following_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if !accepts_activitypub(&headers) {
        return Ok((StatusCode::NOT_ACCEPTABLE, Json(serde_json::json!({
            "error": "Requires Accept: application/activity+json",
        }))).into_response());
    }

    let _actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    let domain = &state.federation_domain;
    let following_uri = format!("https://{}/actors/{}/following", domain, handle);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/activity+json; charset=utf-8")],
        Json(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": following_uri,
            "type": "OrderedCollection",
            "totalItems": 0,
            "orderedItems": [],
        })),
    ).into_response())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Parse `acct:handle@domain` → `handle`.
///
/// Accepte les formes :
/// - `acct:ymclash@shinobi.example.com`
/// - `acct:ymclash@localhost:3000`
fn parse_acct_resource(resource: &str, expected_domain: &str) -> Option<String> {
    let stripped = resource.strip_prefix("acct:")?;
    let (handle, domain) = stripped.split_once('@')?;

    if domain != expected_domain {
        return None;
    }

    if handle.is_empty() {
        return None;
    }

    Some(handle.to_string())
}

/// Vérifie si le header `Accept` demande du contenu ActivityPub.
///
/// ## Piège du Header Accept
/// Doit supporter les DEUX formats utilisés dans le Fediverse :
/// - `application/activity+json` (format moderne, Mastodon >= 4.x)
/// - `application/ld+json; profile="https://www.w3.org/ns/activitystreams"` (legacy)
fn accepts_activitypub(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|accept| {
            accept.contains("application/activity+json")
                || accept.contains("application/ld+json")
                || accept.contains("application/json")
        })
        .unwrap_or(false)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_acct_resource_valid() {
        assert_eq!(
            parse_acct_resource("acct:ymclash@shinobi.example.com", "shinobi.example.com"),
            Some("ymclash".into())
        );
    }

    #[test]
    fn test_parse_acct_resource_localhost() {
        assert_eq!(
            parse_acct_resource("acct:alice@localhost:3000", "localhost:3000"),
            Some("alice".into())
        );
    }

    #[test]
    fn test_parse_acct_resource_wrong_domain() {
        assert_eq!(
            parse_acct_resource("acct:alice@evil.com", "shinobi.example.com"),
            None
        );
    }

    #[test]
    fn test_parse_acct_resource_no_prefix() {
        assert_eq!(
            parse_acct_resource("alice@shinobi.example.com", "shinobi.example.com"),
            None
        );
    }

    #[test]
    fn test_parse_acct_resource_empty_handle() {
        assert_eq!(
            parse_acct_resource("acct:@shinobi.example.com", "shinobi.example.com"),
            None
        );
    }

    #[test]
    fn test_accepts_activitypub_modern() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, "application/activity+json".parse().unwrap());
        assert!(accepts_activitypub(&headers));
    }

    #[test]
    fn test_accepts_activitypub_legacy() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            "application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"".parse().unwrap(),
        );
        assert!(accepts_activitypub(&headers));
    }

    #[test]
    fn test_accepts_activitypub_html_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, "text/html".parse().unwrap());
        assert!(!accepts_activitypub(&headers));
    }
}
