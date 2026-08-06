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

use domain::entities::federation::{FederationActivity, FederationFollow};
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

    // Récupérer ou générer la clé publique (Lazy Keygen — Phase 27-bis-C)
    let public_key_section = match state.federation_repo.get_keypair(&actor.id).await {
        Ok(Some(keypair)) => {
            serde_json::json!({
                "id": keypair.key_id,
                "owner": actor_uri,
                "publicKeyPem": keypair.public_key_pem,
            })
        }
        _ => {
            // Lazy keygen : générer une keypair RSA si elle n'existe pas
            match infrastructure::federation::crypto::generate_rsa_keypair() {
                Ok(kp) => {
                    let key_id = format!("{}#main-key", actor_uri);
                    let fed_kp = domain::entities::federation::FederationKeypair {
                        actor_id: actor.id,
                        public_key_pem: kp.public_key_pem.clone(),
                        private_key_pem: kp.private_key_pem,
                        key_id: key_id.clone(),
                        created_at: Utc::now(),
                    };
                    if let Err(e) = state.federation_repo.save_keypair(&fed_kp).await {
                        warn!(actor_id = %actor.id, error = %e, "⚠️ Failed to save lazy-generated keypair");
                    } else {
                        info!(actor_id = %actor.id, handle = %actor.handle, "🔑 Lazy keygen — keypair generated on first AP fetch");
                    }
                    serde_json::json!({
                        "id": key_id,
                        "owner": actor_uri,
                        "publicKeyPem": kp.public_key_pem,
                    })
                }
                Err(e) => {
                    warn!(error = %e, "⚠️ Lazy keygen failed — publicKey will be null");
                    serde_json::json!(null)
                }
            }
        }
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
/// Reçoit les activités fédérées entrantes.
/// - `Follow` → persiste le follower + répond `Accept` signé (tokio::spawn)
/// - `Undo(Follow)` → supprime le follower
/// - Autres → log + ignore (202 Accepted)
///
/// ## Sécurité (Phase 27-bis-A)
/// 1. Vérifie le clock skew (< 30s) — anti-replay
/// 2. Exige le header `Signature` — Draft-Cavage-12
/// 3. Fetch la clé publique du signataire distant (cache 5 min)
/// 4. Vérifie la signature RSA
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

    // ── Phase 27-bis-A : Vérification de la signature HTTP ──
    let signature_header = headers.get("signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError(DomainError::Unauthorized(
            "Missing Signature header — federation requires HTTP Signatures (Draft-Cavage-12)".into()
        )))?;

    // Parser le header pour extraire le keyId
    let parsed_sig = infrastructure::federation::http_signature::parse_signature_header(signature_header)
        .map_err(|e| AppError(DomainError::Unauthorized(format!("Invalid Signature header: {}", e))))?;

    // Fetch la clé publique distante (avec cache SSRF-guarded)
    let fetcher = infrastructure::federation::remote_actor::RemoteActorFetcher::new();
    let remote_actor = fetcher.fetch(&parsed_sig.key_id.split('#').next().unwrap_or(&parsed_sig.key_id)).await
        .map_err(|e| {
            warn!(key_id = %parsed_sig.key_id, error = %e, "❌ Failed to fetch remote actor for signature verification");
            AppError(DomainError::Unauthorized(format!("Cannot fetch remote signing key: {}", e)))
        })?;

    // Reconstruire la map des headers pour la vérification
    let mut request_headers = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            request_headers.insert(name.as_str().to_lowercase(), v.to_string());
        }
    }

    // Vérifier la signature RSA
    infrastructure::federation::http_signature::verify_signature(
        &remote_actor.public_key_pem,
        signature_header,
        "POST",
        &format!("/actors/{}/inbox", handle),
        &request_headers,
    ).map_err(|e| {
        warn!(key_id = %parsed_sig.key_id, error = %e, "🛡️ Signature verification failed");
        AppError(DomainError::Unauthorized(format!("Invalid HTTP Signature: {}", e)))
    })?;

    info!(key_id = %parsed_sig.key_id, "✅ HTTP Signature verified");

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

            // ── Phase 27-bis-B : Envoyer un Accept signé (Le Facteur) ──
            let domain = state.federation_domain.clone();
            let actor_uri = format!("https://{}/actors/{}", domain, actor.handle);
            let federation_repo = state.federation_repo.clone();
            let actor_id = actor.id;
            let follow_body = body.clone();
            let remote_inbox = remote_actor.inbox.clone();

            tokio::spawn(async move {
                // Récupérer la keypair de l'acteur local
                let keypair = match federation_repo.get_keypair(&actor_id).await {
                    Ok(Some(kp)) => kp,
                    _ => {
                        warn!(actor_id = %actor_id, "⚠️ No keypair for Accept delivery — skipping");
                        return;
                    }
                };

                // Construire l'Accept activity
                let accept = serde_json::json!({
                    "@context": "https://www.w3.org/ns/activitystreams",
                    "id": format!("{}/activities/{}", actor_uri, Uuid::new_v4()),
                    "type": "Accept",
                    "actor": actor_uri,
                    "object": follow_body,
                });

                // Déterminer l'inbox cible
                let target_inbox = if remote_inbox.is_empty() {
                    // Fallback: dériver de l'URI du follower
                    format!("{}/inbox", follower_uri)
                } else {
                    remote_inbox
                };

                // Livrer l'Accept signé
                if let Err(e) = infrastructure::federation::delivery::deliver_activity(
                    accept.clone(),
                    &target_inbox,
                    &keypair.private_key_pem,
                    &keypair.key_id,
                ).await {
                    warn!(target = %target_inbox, error = %e, "⚠️ Accept delivery failed (non-fatal)");
                }

                // Enregistrer l'Accept dans l'outbox (Phase 27-bis-D)
                let activity = FederationActivity {
                    id: Uuid::new_v4(),
                    actor_id,
                    activity_type: "Accept".into(),
                    object_type: "Follow".into(),
                    object_id: follower_uri.clone(),
                    activity_json: accept,
                    published_at: Utc::now(),
                };
                if let Err(e) = federation_repo.save_activity(&activity).await {
                    warn!(error = %e, "⚠️ Failed to save Accept activity to outbox");
                }
            });

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
            // On accepte poliment et on archive (202 Accepted)
            info!(
                activity_type = %activity_type,
                "📋 Activité fédérée non gérée — archivée (stub)"
            );
            Ok((StatusCode::ACCEPTED, Json(serde_json::json!({
                "status": "accepted",
                "type": activity_type,
                "note": "Activity logged but not processed"
            }))).into_response())
        }
    }
}

/// Outbox ActivityPub — `GET /actors/{handle}/outbox`
///
/// Retourne une OrderedCollection des activités récentes.
/// Phase 27-bis-D : activités réelles depuis PostgreSQL.
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
    let actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    let domain = &state.federation_domain;
    let outbox_uri = format!("https://{}/actors/{}/outbox", domain, handle);

    // Phase 27-bis-D : activités réelles
    let activities = state.federation_repo.list_activities(&actor.id, 50).await?;
    let total = state.federation_repo.count_activities(&actor.id).await?;
    let items: Vec<serde_json::Value> = activities.into_iter().map(|a| a.activity_json).collect();

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/activity+json; charset=utf-8")],
        Json(serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": outbox_uri,
            "type": "OrderedCollection",
            "totalItems": total,
            "orderedItems": items,
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
// ── Phase 27-ter — Repository AP Profile
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Profil ActivityPub d'un dépôt — `GET /repos/{owner}/{repo}`
///
/// Rend le `object.id` des activités Create { Repository } et Push
/// résolvable par les forges distantes.
///
/// ## Content Negotiation
/// - `Accept: application/activity+json` → JSON-LD ForgeFed Repository
/// - Autre → 406 Not Acceptable
///
/// ## Route
/// Dédiée sous `/repos/` (hors `/api/v1/`), cohérent avec `/actors/`.
pub async fn repo_ap_handler(
    State(state): State<SharedState>,
    Path((owner, repo_name)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if !accepts_activitypub(&headers) {
        return Ok((
            StatusCode::NOT_ACCEPTABLE,
            Json(serde_json::json!({
                "error": "Not Acceptable",
                "message": "This endpoint requires Accept: application/activity+json",
            })),
        ).into_response());
    }

    info!(owner = %owner, repo = %repo_name, "🌐 ForgeFed Repository profile fetch");

    // Résoudre le repo
    let actor = state
        .actor_repo
        .find_by_handle(&owner)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", owner),
        )))?;

    let repository = state
        .repo_repo
        .find_by_owner_and_name(&actor.id, &repo_name)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Dépôt '{}/{}' introuvable", owner, repo_name),
        )))?;

    let domain = &state.federation_domain;
    let scheme = if domain.contains("localhost") { "http" } else { "https" };
    let repo_uri = format!("{}://{}/repos/{}/{}", scheme, domain, owner, repo_name);
    let actor_uri = format!("{}://{}/actors/{}", scheme, domain, owner);

    let ap_repo = serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://forgefed.org/ns",
        ],
        "type": "Repository",
        "id": repo_uri,
        "name": repository.display_name,
        "summary": repository.description.clone().unwrap_or_default(),
        "attributedTo": actor_uri,
        "published": repository.created_at.to_rfc3339(),
        "url": format!("{}://{}/{}/{}", scheme, domain, owner, repo_name),
        "forkedFrom": serde_json::Value::Null,  // Phase 27-quater
    });

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/activity+json; charset=utf-8")],
        Json(ap_repo),
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
