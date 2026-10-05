//! Port: FederationService — Publication et fanout d'activités fédérées.
//!
//! Ce trait abstrait le mécanisme de publication d'activités ActivityPub
//! dans l'outbox et leur livraison signée aux followers fédérés.
//!
//! ## Architecture hexagonale
//! - Le domain (use cases) dépend de ce **port** (trait).
//! - L'infrastructure implémente le **FanoutService** concret.
//! - `main.rs` assemble le tout (DI).
//!
//! ## Phase 27-ter — Activités Repository Automatiques

use async_trait::async_trait;
use uuid::Uuid;

use crate::errors::DomainError;

/// Service de publication d'activités fédérées (outbox + fanout).
///
/// Implémenté par `FanoutService` dans la couche infrastructure.
/// Injecté en `Option<Arc<dyn FederationService>>` dans les use cases
/// pour le graceful degradation (fédération désactivée = None).
#[async_trait]
pub trait FederationService: Send + Sync {
    /// Publie une activité dans l'outbox et la livre à tous les followers.
    ///
    /// ## Flow
    /// 1. Sauvegarde `FederationActivity` dans PostgreSQL (outbox)
    /// 2. Liste les followers fédérés de l'acteur
    /// 3. Récupère la keypair de l'acteur
    /// 4. Pour chaque follower : livre l'activité signée vers son inbox
    ///
    /// ## Arguments
    /// - `actor_id` : ID de l'acteur local auteur de l'activité
    /// - `activity_type` : Type AP (ex: "Create", "Update", "Push")
    /// - `object_type` : Type de l'objet (ex: "Repository")
    /// - `object_id` : URI de l'objet (ex: "https://domain/repos/owner/name")
    /// - `activity_json` : Activité AP complète en JSON-LD
    async fn publish_activity(
        &self,
        actor_id: &Uuid,
        activity_type: &str,
        object_type: &str,
        object_id: &str,
        activity_json: serde_json::Value,
    ) -> Result<(), DomainError>;
}
