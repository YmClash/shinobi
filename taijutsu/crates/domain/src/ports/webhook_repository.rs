//! Port: WebhookRepository — Phase 34 (Chakra チャクラ)
//!
//! Contrat d'accès aux webhooks et à leur historique de livraison.
//! L'adaptateur concret dans infrastructure/ implémente le stockage PostgreSQL.
//!
//! ## Séparation des responsabilités
//! - **CRUD webhooks** : Géré par l'API REST (ManageWebhooksUseCase)
//! - **Lookup actif** : Utilisé par le consumer Chakra pour trouver
//!   les endpoints abonnés à un événement donné
//! - **Deliveries** : Audit trail pour le dashboard développeur

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::webhook::{Webhook, WebhookDelivery};
use crate::errors::DomainError;

/// Contrat d'accès aux webhooks et à leur historique de livraison.
#[async_trait]
pub trait WebhookRepository: Send + Sync {
    // ── CRUD Webhooks ─────────────────────────────────────────────

    /// Persiste un nouveau webhook.
    async fn save(&self, webhook: &Webhook) -> Result<(), DomainError>;

    /// Trouve un webhook par son ID.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Webhook>, DomainError>;

    /// Liste tous les webhooks d'un dépôt (actifs et inactifs).
    async fn list_by_repository(&self, repository_id: &Uuid) -> Result<Vec<Webhook>, DomainError>;

    /// Trouve les webhooks **actifs** abonnés à un type d'événement donné.
    /// C'est la query critique du consumer Chakra — doit être rapide.
    /// Utilise l'index partiel `WHERE active = TRUE`.
    async fn find_active_for_event(
        &self,
        repository_id: &Uuid,
        event_type: &str,
    ) -> Result<Vec<Webhook>, DomainError>;

    /// Met à jour un webhook existant (URL, events, active).
    async fn update(&self, webhook: &Webhook) -> Result<(), DomainError>;

    /// Supprime un webhook par son ID.
    async fn delete(&self, id: &Uuid) -> Result<(), DomainError>;

    /// Compte le nombre de webhooks d'un dépôt (pour vérifier la limite).
    async fn count_by_repository(&self, repository_id: &Uuid) -> Result<i64, DomainError>;

    /// Incrémente le compteur d'échecs consécutifs d'un webhook.
    /// Reset à 0 si `reset` est true (après une livraison réussie).
    async fn update_failure_count(
        &self,
        webhook_id: &Uuid,
        reset: bool,
    ) -> Result<(), DomainError>;

    /// Met à jour la date de dernière livraison réussie.
    async fn update_last_delivery(
        &self,
        webhook_id: &Uuid,
    ) -> Result<(), DomainError>;

    // ── Deliveries (Historique) ────────────────────────────────────

    /// Persiste une livraison webhook (audit trail).
    async fn save_delivery(&self, delivery: &WebhookDelivery) -> Result<(), DomainError>;

    /// Liste les livraisons d'un webhook (ordre décroissant).
    async fn list_deliveries(
        &self,
        webhook_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, DomainError>;

    /// Trouve une livraison par son ID.
    async fn find_delivery_by_id(
        &self,
        delivery_id: &Uuid,
    ) -> Result<Option<WebhookDelivery>, DomainError>;
}
