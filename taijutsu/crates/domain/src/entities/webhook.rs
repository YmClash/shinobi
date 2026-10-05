//! Entité Webhook — Phase 34 (Chakra チャクラ) 🔔
//!
//! Représente un abonnement webhook d'un dépôt vers un endpoint HTTP externe.
//! Permet aux développeurs de brancher Drone CI, Woodpecker CI ou Jenkins
//! en quelques clics.
//!
//! ## Sécurité
//! - Chaque webhook possède un `secret` HMAC-SHA256 généré côté serveur.
//! - Le payload est signé avec ce secret → header `X-Shinobi-Signature`.
//! - Protection SSRF : les URLs privées (localhost, 10.x, 172.x, 192.168.x) sont bloquées.
//!
//! ## CloudEvents
//! Les événements suivent le standard CNCF CloudEvents 1.0 pour l'interopérabilité.
//! Les headers `ce-*` sont envoyés en plus des headers `X-Shinobi-*`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Webhook ───────────────────────────────────────────────────────────

/// Abonnement webhook d'un dépôt vers un endpoint HTTP externe.
///
/// Un webhook est activé pour un ou plusieurs types d'événements.
/// À chaque événement, le payload est signé HMAC-SHA256 et envoyé
/// en POST vers l'URL configurée.
#[derive(Debug, Clone)]
pub struct Webhook {
    /// Identifiant unique.
    pub id: Uuid,
    /// Dépôt source des événements.
    pub repository_id: Uuid,
    /// Acteur ayant créé le webhook.
    pub creator_id: Uuid,
    /// URL cible du POST (HTTPS obligatoire en production).
    pub url: String,
    /// Secret HMAC-SHA256 pour la signature des payloads.
    /// Généré côté serveur (32 bytes random, hex-encoded → 64 chars).
    /// Renvoyé uniquement à la création.
    pub secret: String,
    /// Types d'événements auxquels ce webhook est abonné.
    pub events: Vec<WebhookEventType>,
    /// Webhook actif (peut être désactivé temporairement).
    pub active: bool,
    /// Dernière livraison réussie (pour monitoring dashboard).
    pub last_delivery_at: Option<DateTime<Utc>>,
    /// Compteur d'échecs consécutifs (auto-disable après N échecs).
    pub failure_count: i32,
    /// Date de création.
    pub created_at: DateTime<Utc>,
    /// Date de dernière modification.
    pub updated_at: DateTime<Utc>,
}

impl Webhook {
    /// Construit un nouveau webhook avec un secret auto-généré.
    pub fn new(
        repository_id: Uuid,
        creator_id: Uuid,
        url: String,
        events: Vec<WebhookEventType>,
    ) -> Self {
        let secret = generate_webhook_secret();
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            repository_id,
            creator_id,
            url,
            secret,
            events,
            active: true,
            last_delivery_at: None,
            failure_count: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Génère un secret HMAC aléatoire (32 bytes → 64 chars hex).
pub fn generate_webhook_secret() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex::encode(bytes)
}

// ── WebhookEventType ──────────────────────────────────────────────────

/// Types d'événements webhook — extensible pour les phases futures.
///
/// ## CloudEvents Type Mapping
/// Chaque variant correspond à un type CloudEvents `dev.jjshinobi.*` :
/// - `Push` → `dev.jjshinobi.push`
/// - `MrCreated` → `dev.jjshinobi.merge_request.created`
/// - etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    /// git push (nouvelles opérations VCS).
    Push,
    /// Merge Request créée.
    MrCreated,
    /// Merge Request fusionnée.
    MrMerged,
    /// Merge Request fermée (sans merge).
    MrClosed,
    /// Issue ouverte.
    IssueOpened,
    /// Issue fermée.
    IssueClosed,
    /// Commentaire sur une issue.
    IssueComment,
    /// Phase 34-V3 — Checkpoint ANBU (preuve de provenance IA).
    AnbuCheckpoint,
}

impl WebhookEventType {
    /// Conversion vers la valeur SQL enum.
    pub fn as_sql_str(&self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::MrCreated => "mr_created",
            Self::MrMerged => "mr_merged",
            Self::MrClosed => "mr_closed",
            Self::IssueOpened => "issue_opened",
            Self::IssueClosed => "issue_closed",
            Self::IssueComment => "issue_comment",
            Self::AnbuCheckpoint => "anbu_checkpoint",
        }
    }

    /// Reconstruction depuis la valeur SQL enum.
    pub fn from_sql_str(s: &str) -> Option<Self> {
        match s {
            "push" => Some(Self::Push),
            "mr_created" => Some(Self::MrCreated),
            "mr_merged" => Some(Self::MrMerged),
            "mr_closed" => Some(Self::MrClosed),
            "issue_opened" => Some(Self::IssueOpened),
            "issue_closed" => Some(Self::IssueClosed),
            "issue_comment" => Some(Self::IssueComment),
            "anbu_checkpoint" => Some(Self::AnbuCheckpoint),
            _ => None,
        }
    }

    /// Type CloudEvents CNCF (namespace `dev.jjshinobi`).
    ///
    /// Suit la convention CloudEvents : reverse-DNS + event category.
    pub fn as_cloudevent_type(&self) -> &'static str {
        match self {
            Self::Push => "dev.jjshinobi.push",
            Self::MrCreated => "dev.jjshinobi.merge_request.created",
            Self::MrMerged => "dev.jjshinobi.merge_request.merged",
            Self::MrClosed => "dev.jjshinobi.merge_request.closed",
            Self::IssueOpened => "dev.jjshinobi.issue.opened",
            Self::IssueClosed => "dev.jjshinobi.issue.closed",
            Self::IssueComment => "dev.jjshinobi.issue.commented",
            Self::AnbuCheckpoint => "dev.jjshinobi.anbu.checkpoint",
        }
    }

    /// Tous les types d'événements disponibles (pour validation).
    pub fn all() -> &'static [Self] {
        &[
            Self::Push,
            Self::MrCreated,
            Self::MrMerged,
            Self::MrClosed,
            Self::IssueOpened,
            Self::IssueClosed,
            Self::IssueComment,
            Self::AnbuCheckpoint,
        ]
    }
}

impl std::fmt::Display for WebhookEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql_str())
    }
}

// ── WebhookEvent ──────────────────────────────────────────────────────

/// Événement webhook à dispatcher via Kafka.
///
/// Sérialisé en JSON et publié sur le topic `shinobi.events.webhooks`.
/// Le consumer Chakra le désérialise, cherche les webhooks abonnés,
/// et dispatch les requêtes HTTP signées.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Identifiant unique de l'événement.
    pub id: Uuid,
    /// Type d'événement.
    pub event_type: WebhookEventType,
    /// Dépôt source de l'événement.
    pub repository_id: Uuid,
    /// Acteur ayant déclenché l'événement.
    pub actor_id: Uuid,
    /// Payload standardisé (détails spécifiques au type d'événement).
    /// Compatible GitHub/Gitea pour faciliter l'intégration CI/CD.
    pub payload: serde_json::Value,
    /// Horodatage de création.
    pub created_at: DateTime<Utc>,
}

impl WebhookEvent {
    /// Construit un nouvel événement webhook.
    pub fn new(
        event_type: WebhookEventType,
        repository_id: Uuid,
        actor_id: Uuid,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            repository_id,
            actor_id,
            payload,
            created_at: Utc::now(),
        }
    }
}

// ── WebhookDelivery ───────────────────────────────────────────────────

/// Historique d'une livraison de webhook — audit trail complet.
///
/// Chaque tentative (y compris les retries) génère une entrée distincte.
/// Permet au développeur de diagnostiquer les échecs dans le dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookDelivery {
    /// Identifiant unique de la livraison.
    pub id: Uuid,
    /// Webhook ayant généré cette livraison.
    pub webhook_id: Uuid,
    /// Type d'événement.
    pub event_type: WebhookEventType,
    /// Identifiant de l'événement source.
    pub event_id: Uuid,
    /// URL cible au moment de la livraison.
    pub url: String,
    /// Headers de la requête envoyée (pour debug).
    pub request_headers: serde_json::Value,
    /// Body de la requête envoyée.
    pub request_body: String,
    /// Code HTTP de la réponse (None si timeout/erreur réseau).
    pub response_status: Option<i16>,
    /// Body de la réponse (tronqué à 10KB).
    pub response_body: Option<String>,
    /// Headers de la réponse.
    pub response_headers: Option<serde_json::Value>,
    /// Durée de la requête en millisecondes.
    pub duration_ms: Option<i64>,
    /// Livraison réussie (2xx).
    pub success: bool,
    /// Numéro de tentative (1 = première, 2-5 = retries).
    pub attempt: i16,
    /// Message d'erreur (timeout, DNS, etc.).
    pub error_message: Option<String>,
    /// Horodatage de la livraison.
    pub created_at: DateTime<Utc>,
}

impl WebhookDelivery {
    /// Construit une nouvelle livraison (résultat à remplir après le POST).
    pub fn new(
        webhook_id: Uuid,
        event_type: WebhookEventType,
        event_id: Uuid,
        url: String,
        request_headers: serde_json::Value,
        request_body: String,
        attempt: i16,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            webhook_id,
            event_type,
            event_id,
            url,
            request_headers,
            request_body,
            response_status: None,
            response_body: None,
            response_headers: None,
            duration_ms: None,
            success: false,
            attempt,
            error_message: None,
            created_at: Utc::now(),
        }
    }

    /// Marque la livraison comme réussie.
    pub fn mark_success(
        &mut self,
        status: i16,
        body: Option<String>,
        headers: Option<serde_json::Value>,
        duration_ms: i64,
    ) {
        self.response_status = Some(status);
        self.response_body = body;
        self.response_headers = headers;
        self.duration_ms = Some(duration_ms);
        self.success = true;
    }

    /// Marque la livraison comme échouée.
    pub fn mark_failure(
        &mut self,
        status: Option<i16>,
        body: Option<String>,
        duration_ms: Option<i64>,
        error: String,
    ) {
        self.response_status = status;
        self.response_body = body;
        self.duration_ms = duration_ms;
        self.success = false;
        self.error_message = Some(error);
    }
}

// ── Retry Payload ─────────────────────────────────────────────────────

/// Payload stocké dans Redis pour les retries.
///
/// Sérialisé en JSON et stocké dans le ZSET `shinobi:chakra:retries`
/// avec le timestamp de la prochaine tentative comme score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRetryPayload {
    /// Webhook ID pour le lookup du secret.
    pub webhook_id: Uuid,
    /// Événement original.
    pub event: WebhookEvent,
    /// URL cible.
    pub url: String,
    /// Secret HMAC.
    pub secret: String,
    /// Numéro de la prochaine tentative (2-5).
    pub next_attempt: i16,
}

impl WebhookRetryPayload {
    /// Calcule le délai de backoff exponentiel pour la tentative donnée.
    ///
    /// | Tentative | Délai |
    /// |---|---|
    /// | 2 | 60s (1 min) |
    /// | 3 | 300s (5 min) |
    /// | 4 | 1800s (30 min) |
    /// | 5 | 7200s (2 heures) |
    pub fn backoff_seconds(attempt: i16) -> u64 {
        match attempt {
            2 => 60,
            3 => 300,
            4 => 1800,
            5 => 7200,
            _ => 7200, // Cap à 2h pour toute tentative > 5
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_webhook_generates_secret() {
        let wh = Webhook::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "https://ci.example.com/hooks".to_string(),
            vec![WebhookEventType::Push],
        );
        assert!(wh.active);
        assert_eq!(wh.secret.len(), 64); // 32 bytes hex
        assert_eq!(wh.failure_count, 0);
        assert!(wh.last_delivery_at.is_none());
    }

    #[test]
    fn test_webhook_event_type_roundtrip() {
        for event_type in WebhookEventType::all() {
            let sql = event_type.as_sql_str();
            let parsed = WebhookEventType::from_sql_str(sql).unwrap();
            assert_eq!(*event_type, parsed);
        }
    }

    #[test]
    fn test_webhook_event_type_serde_json() {
        let json = serde_json::to_string(&WebhookEventType::Push).unwrap();
        assert_eq!(json, "\"push\"");
        let deserialized: WebhookEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, WebhookEventType::Push);
    }

    #[test]
    fn test_webhook_event_type_cloudevents() {
        assert_eq!(WebhookEventType::Push.as_cloudevent_type(), "dev.jjshinobi.push");
        assert_eq!(
            WebhookEventType::MrCreated.as_cloudevent_type(),
            "dev.jjshinobi.merge_request.created"
        );
        assert_eq!(
            WebhookEventType::IssueOpened.as_cloudevent_type(),
            "dev.jjshinobi.issue.opened"
        );
    }

    #[test]
    fn test_webhook_event_serialization() {
        let event = WebhookEvent::new(
            WebhookEventType::Push,
            Uuid::new_v4(),
            Uuid::new_v4(),
            serde_json::json!({"ref": "refs/heads/main"}),
        );
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: WebhookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.id, deserialized.id);
        assert_eq!(event.event_type, deserialized.event_type);
    }

    #[test]
    fn test_delivery_mark_success() {
        let mut delivery = WebhookDelivery::new(
            Uuid::new_v4(),
            WebhookEventType::Push,
            Uuid::new_v4(),
            "https://ci.example.com".to_string(),
            serde_json::json!({}),
            "{}".to_string(),
            1,
        );
        assert!(!delivery.success);

        delivery.mark_success(200, Some("OK".to_string()), None, 42);
        assert!(delivery.success);
        assert_eq!(delivery.response_status, Some(200));
        assert_eq!(delivery.duration_ms, Some(42));
    }

    #[test]
    fn test_delivery_mark_failure() {
        let mut delivery = WebhookDelivery::new(
            Uuid::new_v4(),
            WebhookEventType::Push,
            Uuid::new_v4(),
            "https://ci.example.com".to_string(),
            serde_json::json!({}),
            "{}".to_string(),
            1,
        );
        delivery.mark_failure(Some(500), Some("Internal Server Error".to_string()), Some(100), "Server error".to_string());
        assert!(!delivery.success);
        assert_eq!(delivery.error_message, Some("Server error".to_string()));
    }

    #[test]
    fn test_backoff_seconds() {
        assert_eq!(WebhookRetryPayload::backoff_seconds(2), 60);
        assert_eq!(WebhookRetryPayload::backoff_seconds(3), 300);
        assert_eq!(WebhookRetryPayload::backoff_seconds(4), 1800);
        assert_eq!(WebhookRetryPayload::backoff_seconds(5), 7200);
    }

    #[test]
    fn test_webhook_event_type_from_invalid() {
        assert!(WebhookEventType::from_sql_str("invalid").is_none());
    }

    #[test]
    fn test_unique_secrets() {
        let wh1 = Webhook::new(Uuid::new_v4(), Uuid::new_v4(), "https://a.com".to_string(), vec![]);
        let wh2 = Webhook::new(Uuid::new_v4(), Uuid::new_v4(), "https://b.com".to_string(), vec![]);
        assert_ne!(wh1.secret, wh2.secret, "Each webhook must have a unique secret");
    }
}
