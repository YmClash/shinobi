//! Use Case : Gestion CRUD des webhooks — Phase 34 (Chakra チャクラ)
//!
//! Permet aux développeurs de créer, lister, modifier et supprimer
//! les webhooks de leurs dépôts via l'API REST.
//!
//! ## Sécurité
//! - Vérification RBAC : seul le owner du repo peut CRUD les webhooks
//! - Le secret HMAC est auto-généré (jamais fourni par le client)
//! - Validation SSRF à la création (URL sûre uniquement)
//! - Limite de webhooks par repo (configurable, défaut 20)

use std::net::IpAddr;
use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use domain::entities::webhook::{Webhook, WebhookDelivery, WebhookEventType};
use domain::errors::DomainError;
use domain::ports::actor_repository::ActorRepository;
use domain::ports::repo_repository::RepoRepository;
use domain::ports::webhook_repository::WebhookRepository;

/// Use case CRUD pour les webhooks.
pub struct ManageWebhooksUseCase {
    webhook_repo: Arc<dyn WebhookRepository>,
    repo_repo: Arc<dyn RepoRepository>,
    actor_repo: Arc<dyn ActorRepository>,
    /// Nombre maximum de webhooks par dépôt.
    max_per_repo: usize,
    /// Autoriser les webhooks vers localhost (dev only).
    allow_local: bool,
}

impl ManageWebhooksUseCase {
    /// Construit le use case.
    pub fn new(
        webhook_repo: Arc<dyn WebhookRepository>,
        repo_repo: Arc<dyn RepoRepository>,
        actor_repo: Arc<dyn ActorRepository>,
        max_per_repo: usize,
        allow_local: bool,
    ) -> Self {
        Self {
            webhook_repo,
            repo_repo,
            actor_repo,
            max_per_repo,
            allow_local,
        }
    }

    /// Crée un nouveau webhook pour un dépôt.
    ///
    /// # Validations
    /// - Le dépôt existe et appartient à l'acteur
    /// - L'URL est sûre (anti-SSRF)
    /// - La limite de webhooks par repo n'est pas atteinte
    /// - Les types d'événements sont valides
    pub async fn create_webhook(
        &self,
        actor_id: Uuid,
        owner: &str,
        repo_name: &str,
        url: String,
        events: Vec<String>,
    ) -> Result<Webhook, DomainError> {
        // ── 1. Résoudre le owner handle et vérifier l'ownership ──
        let owner_actor = self
            .actor_repo
            .find_by_handle(owner)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        let repo = self
            .repo_repo
            .find_by_owner_and_name(&owner_actor.id, repo_name)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: Uuid::nil(),
            })?;

        if repo.owner_id != actor_id {
            return Err(DomainError::Forbidden(
                "Only the repository owner can manage webhooks".to_string(),
            ));
        }

        // ── 2. Vérifier la limite ────────────────────────────────
        let count = self.webhook_repo.count_by_repository(&repo.id).await?;
        if count >= self.max_per_repo as i64 {
            return Err(DomainError::BusinessRule(format!(
                "Maximum {} webhooks per repository reached",
                self.max_per_repo
            )));
        }

        // ── 3. Valider l'URL (anti-SSRF) ─────────────────────────
        if !is_safe_webhook_url(&url, self.allow_local) {
            return Err(DomainError::BusinessRule(
                "Webhook URL is not allowed (SSRF protection: private networks and non-HTTPS URLs are blocked)".to_string(),
            ));
        }

        // ── 4. Parser les types d'événements ─────────────────────
        let event_types: Vec<WebhookEventType> = events
            .iter()
            .filter_map(|e| WebhookEventType::from_sql_str(e))
            .collect();

        if event_types.is_empty() {
            return Err(DomainError::BusinessRule(
                "At least one valid event type is required".to_string(),
            ));
        }

        // ── 5. Créer le webhook ──────────────────────────────────
        let webhook = Webhook::new(repo.id, actor_id, url, event_types);

        self.webhook_repo.save(&webhook).await?;

        info!(
            webhook_id = %webhook.id,
            repository_id = %repo.id,
            url = %webhook.url,
            events = ?webhook.events,
            "🔔 Chakra — Webhook créé"
        );

        Ok(webhook)
    }

    /// Liste les webhooks d'un dépôt.
    pub async fn list_webhooks(
        &self,
        actor_id: Uuid,
        owner: &str,
        repo_name: &str,
    ) -> Result<Vec<Webhook>, DomainError> {
        let owner_actor = self
            .actor_repo
            .find_by_handle(owner)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Actor",
                id: Uuid::nil(),
            })?;

        let repo = self
            .repo_repo
            .find_by_owner_and_name(&owner_actor.id, repo_name)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: Uuid::nil(),
            })?;

        if repo.owner_id != actor_id {
            return Err(DomainError::Forbidden(
                "Only the repository owner can view webhooks".to_string(),
            ));
        }

        self.webhook_repo.list_by_repository(&repo.id).await
    }

    /// Récupère un webhook par son ID.
    pub async fn get_webhook(
        &self,
        actor_id: Uuid,
        webhook_id: Uuid,
    ) -> Result<Webhook, DomainError> {
        let webhook = self
            .webhook_repo
            .find_by_id(&webhook_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Webhook",
                id: webhook_id,
            })?;

        // Vérifier l'ownership du repo
        let repo = self
            .repo_repo
            .find_by_id(&webhook.repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Repository",
                id: webhook.repository_id,
            })?;

        if repo.owner_id != actor_id {
            return Err(DomainError::Forbidden(
                "Only the repository owner can view webhooks".to_string(),
            ));
        }

        Ok(webhook)
    }

    /// Modifie un webhook existant (URL, events, active).
    pub async fn update_webhook(
        &self,
        actor_id: Uuid,
        webhook_id: Uuid,
        url: Option<String>,
        events: Option<Vec<String>>,
        active: Option<bool>,
    ) -> Result<Webhook, DomainError> {
        let mut webhook = self.get_webhook(actor_id, webhook_id).await?;

        if let Some(new_url) = url {
            if !is_safe_webhook_url(&new_url, self.allow_local) {
                return Err(DomainError::BusinessRule(
                    "Webhook URL is not allowed (SSRF protection)".to_string(),
                ));
            }
            webhook.url = new_url;
        }

        if let Some(new_events) = events {
            let event_types: Vec<WebhookEventType> = new_events
                .iter()
                .filter_map(|e| WebhookEventType::from_sql_str(e))
                .collect();
            if event_types.is_empty() {
                return Err(DomainError::BusinessRule(
                    "At least one valid event type is required".to_string(),
                ));
            }
            webhook.events = event_types;
        }

        if let Some(new_active) = active {
            webhook.active = new_active;
        }

        webhook.updated_at = chrono::Utc::now();
        self.webhook_repo.update(&webhook).await?;

        info!(
            webhook_id = %webhook.id,
            "🔔 Chakra — Webhook mis à jour"
        );

        Ok(webhook)
    }

    /// Supprime un webhook.
    pub async fn delete_webhook(
        &self,
        actor_id: Uuid,
        webhook_id: Uuid,
    ) -> Result<(), DomainError> {
        // Vérification ownership via get_webhook
        let _ = self.get_webhook(actor_id, webhook_id).await?;

        self.webhook_repo.delete(&webhook_id).await?;

        info!(webhook_id = %webhook_id, "🔔 Chakra — Webhook supprimé");

        Ok(())
    }

    /// Régénère le secret HMAC d'un webhook.
    ///
    /// Le nouveau secret est retourné dans la réponse (affiché une seule fois).
    /// L'ancien secret est invalidé immédiatement.
    pub async fn regenerate_secret(
        &self,
        actor_id: Uuid,
        webhook_id: Uuid,
    ) -> Result<Webhook, DomainError> {
        let mut webhook = self.get_webhook(actor_id, webhook_id).await?;

        // Utiliser la fonction domaine pour générer le nouveau secret
        webhook.secret = domain::entities::webhook::generate_webhook_secret();
        webhook.updated_at = chrono::Utc::now();

        self.webhook_repo.update(&webhook).await?;

        info!(
            webhook_id = %webhook.id,
            "🔔 Chakra — Secret webhook régénéré"
        );

        Ok(webhook)
    }

    /// Liste l'historique des livraisons d'un webhook.
    pub async fn list_deliveries(
        &self,
        actor_id: Uuid,
        webhook_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DomainError> {
        // Vérification ownership
        let _ = self.get_webhook(actor_id, webhook_id).await?;

        self.webhook_repo.list_deliveries(&webhook_id, limit).await
    }
}

// ── SSRF Validation ───────────────────────────────────────────────────

/// Vérifie qu'une URL webhook est sûre (anti-SSRF).
///
/// Identique à la vérification du dispatcher mais côté use case
/// pour rejeter les URLs dangereuses dès la création.
fn is_safe_webhook_url(url: &str, allow_local: bool) -> bool {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let scheme = parsed.scheme();
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return false,
    };

    // HTTPS obligatoire (sauf localhost en mode dev)
    if scheme != "https" {
        if allow_local && scheme == "http" && is_localhost(host) {
            // OK en dev
        } else {
            return false;
        }
    }

    // Domaines internes bloqués
    if host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".localhost")
    {
        return false;
    }

    // Vérification IP
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(&ip) {
            if allow_local && ip.is_loopback() {
                return true;
            }
            return false;
        }
    }

    // Hostname = "localhost"
    if is_localhost(host) {
        return allow_local;
    }

    true
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

fn is_localhost(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1"
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_url_https() {
        assert!(is_safe_webhook_url("https://ci.example.com/hooks", false));
        assert!(is_safe_webhook_url("https://drone.io/webhook", false));
    }

    #[test]
    fn test_unsafe_url_http() {
        assert!(!is_safe_webhook_url("http://ci.example.com/hooks", false));
    }

    #[test]
    fn test_unsafe_url_private_networks() {
        assert!(!is_safe_webhook_url("https://127.0.0.1/hooks", false));
        assert!(!is_safe_webhook_url("https://10.0.0.1/hooks", false));
        assert!(!is_safe_webhook_url("https://172.16.0.1/hooks", false));
        assert!(!is_safe_webhook_url("https://192.168.1.1/hooks", false));
    }

    #[test]
    fn test_unsafe_url_localhost() {
        assert!(!is_safe_webhook_url("https://localhost/hooks", false));
        assert!(!is_safe_webhook_url("http://localhost:3000/hooks", false));
    }

    #[test]
    fn test_safe_url_localhost_dev_mode() {
        assert!(is_safe_webhook_url("http://localhost:3000/hooks", true));
        assert!(is_safe_webhook_url("http://127.0.0.1:8080/hooks", true));
    }

    #[test]
    fn test_unsafe_url_internal_domains() {
        assert!(!is_safe_webhook_url("https://redis.local/hooks", false));
        assert!(!is_safe_webhook_url("https://db.internal/hooks", false));
    }

    #[test]
    fn test_unsafe_url_invalid() {
        assert!(!is_safe_webhook_url("not-a-url", false));
        assert!(!is_safe_webhook_url("", false));
        assert!(!is_safe_webhook_url("ftp://example.com", false));
    }
}
