//! Routes REST — Webhooks (Phase 34 — Chakra チャクラ) 🔔
//!
//! API CRUD pour la gestion des webhooks + historique des livraisons.
//! Toutes les routes nécessitent une authentification JWT.
//!
//! ## Endpoints
//! | Méthode | Route | Description |
//! |---|---|---|
//! | POST | `/api/v1/repos/:owner/:repo/hooks` | Créer un webhook |
//! | GET | `/api/v1/repos/:owner/:repo/hooks` | Lister les webhooks |
//! | GET | `/api/v1/repos/:owner/:repo/hooks/:id` | Détail d'un webhook |
//! | PATCH | `/api/v1/repos/:owner/:repo/hooks/:id` | Modifier un webhook |
//! | DELETE | `/api/v1/repos/:owner/:repo/hooks/:id` | Supprimer un webhook |
//! | GET | `/api/v1/repos/:owner/:repo/hooks/:id/deliveries` | Historique |
//! | POST | `/api/v1/repos/:owner/:repo/hooks/:id/ping` | Ping de test |

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use domain::entities::webhook::{Webhook, WebhookDelivery, WebhookEventType};

use crate::errors::AppError;
use crate::rest::auth_middleware::AuthUser;
use crate::state::SharedState;

// ── Request/Response Types ────────────────────────────────────────────

/// Corps de la requête POST /hooks (création).
#[derive(Debug, Deserialize)]
pub struct CreateWebhookBody {
    /// URL cible du webhook (HTTPS obligatoire en prod).
    pub url: String,
    /// Types d'événements auxquels s'abonner.
    /// Valeurs: "push", "mr_created", "mr_merged", "mr_closed",
    ///          "issue_opened", "issue_closed", "issue_comment"
    pub events: Vec<String>,
    /// Webhook actif dès la création (défaut: true).
    pub active: Option<bool>,
}

/// Corps de la requête PATCH /hooks/:id (mise à jour).
#[derive(Debug, Deserialize)]
pub struct UpdateWebhookBody {
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    pub active: Option<bool>,
}

/// Réponse webhook (le secret n'est renvoyé qu'à la création).
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    /// Secret HMAC-SHA256 — renvoyé UNIQUEMENT à la création.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    pub last_delivery_at: Option<String>,
    pub failure_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl WebhookResponse {
    /// Conversion depuis l'entité domaine (sans secret).
    fn from_webhook(wh: &Webhook) -> Self {
        Self {
            id: wh.id,
            url: wh.url.clone(),
            events: wh.events.iter().map(|e| e.as_sql_str().to_string()).collect(),
            active: wh.active,
            secret: None,
            last_delivery_at: wh.last_delivery_at.map(|d| d.to_rfc3339()),
            failure_count: wh.failure_count,
            created_at: wh.created_at.to_rfc3339(),
            updated_at: wh.updated_at.to_rfc3339(),
        }
    }

    /// Conversion avec le secret (pour la réponse de création).
    fn from_webhook_with_secret(wh: &Webhook) -> Self {
        let mut resp = Self::from_webhook(wh);
        resp.secret = Some(wh.secret.clone());
        resp
    }
}

/// Réponse d'une livraison webhook.
#[derive(Debug, Serialize)]
pub struct DeliveryResponse {
    pub id: Uuid,
    pub event_type: String,
    pub event_id: Uuid,
    pub url: String,
    pub response_status: Option<i16>,
    pub success: bool,
    pub attempt: i16,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: String,
}

impl DeliveryResponse {
    fn from_delivery(d: &WebhookDelivery) -> Self {
        Self {
            id: d.id,
            event_type: d.event_type.as_sql_str().to_string(),
            event_id: d.event_id,
            url: d.url.clone(),
            response_status: d.response_status,
            success: d.success,
            attempt: d.attempt,
            duration_ms: d.duration_ms,
            error_message: d.error_message.clone(),
            created_at: d.created_at.to_rfc3339(),
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────

/// POST /api/v1/repos/:owner/:repo/hooks — Créer un webhook
pub(crate) async fn create_webhook_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    Json(body): Json<CreateWebhookBody>,
) -> Result<Json<WebhookResponse>, AppError> {
    let webhook = state
        .manage_webhooks
        .create_webhook(auth.0.actor_id(), &owner, &repo, body.url, body.events)
        .await?;

    Ok(Json(WebhookResponse::from_webhook_with_secret(&webhook)))
}

/// GET /api/v1/repos/:owner/:repo/hooks — Lister les webhooks
pub(crate) async fn list_webhooks_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Json<Vec<WebhookResponse>>, AppError> {
    let webhooks = state
        .manage_webhooks
        .list_webhooks(auth.0.actor_id(), &owner, &repo)
        .await?;

    let response: Vec<WebhookResponse> = webhooks
        .iter()
        .map(WebhookResponse::from_webhook)
        .collect();

    Ok(Json(response))
}

/// GET /api/v1/repos/:owner/:repo/hooks/:id — Détail d'un webhook
pub(crate) async fn get_webhook_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((_owner, _repo, id)): Path<(String, String, Uuid)>,
) -> Result<Json<WebhookResponse>, AppError> {
    let webhook = state
        .manage_webhooks
        .get_webhook(auth.0.actor_id(), id)
        .await?;

    Ok(Json(WebhookResponse::from_webhook(&webhook)))
}

/// PATCH /api/v1/repos/:owner/:repo/hooks/:id — Modifier un webhook
pub(crate) async fn update_webhook_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((_owner, _repo, id)): Path<(String, String, Uuid)>,
    Json(body): Json<UpdateWebhookBody>,
) -> Result<Json<WebhookResponse>, AppError> {
    let webhook = state
        .manage_webhooks
        .update_webhook(auth.0.actor_id(), id, body.url, body.events, body.active)
        .await?;

    Ok(Json(WebhookResponse::from_webhook(&webhook)))
}

/// DELETE /api/v1/repos/:owner/:repo/hooks/:id — Supprimer un webhook
pub(crate) async fn delete_webhook_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((_owner, _repo, id)): Path<(String, String, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .manage_webhooks
        .delete_webhook(auth.0.actor_id(), id)
        .await?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// GET /api/v1/repos/:owner/:repo/hooks/:id/deliveries — Historique des livraisons
pub(crate) async fn list_deliveries_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((_owner, _repo, id)): Path<(String, String, Uuid)>,
) -> Result<Json<Vec<DeliveryResponse>>, AppError> {
    let deliveries = state
        .manage_webhooks
        .list_deliveries(auth.0.actor_id(), id, 50)
        .await?;

    let response: Vec<DeliveryResponse> = deliveries
        .iter()
        .map(DeliveryResponse::from_delivery)
        .collect();

    Ok(Json(response))
}

/// POST /api/v1/repos/:owner/:repo/hooks/:id/ping — Ping de test
pub(crate) async fn ping_webhook_handler(
    auth: AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, id)): Path<(String, String, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Vérifier que le webhook existe et appartient à l'acteur
    let _webhook = state
        .manage_webhooks
        .get_webhook(auth.0.actor_id(), id)
        .await?;

    // Émettre un événement de test
    if let Some(ref emitter) = state.emit_webhook {
        let payload = serde_json::json!({
            "action": "ping",
            "sender": {
                "id": auth.0.actor_id(),
                "handle": auth.0.handle,
            },
            "hook_id": id,
            "repository": {
                "owner": owner,
                "name": repo,
            }
        });

        emitter.emit(
            WebhookEventType::Push, // Ping uses push event type
            _webhook.repository_id,
            auth.0.actor_id(),
            payload,
        ).await;
    }

    Ok(Json(serde_json::json!({ "pong": true })))
}
