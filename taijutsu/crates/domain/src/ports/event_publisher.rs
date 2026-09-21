//! Port: EventPublisher — Contrat de publication événementielle (Nen).
//!
//! Ce trait abstrait le bus d'événements sous-jacent (Kafka, in-memory, etc.).
//! L'adaptateur concret dans infrastructure/ implémente le transport réel.
//!
//! ## Pattern
//! Fire-and-Forget avec gestion d'erreur : l'appelant est notifié si la
//! publication échoue, mais l'opération VCS n'est PAS annulée (at-most-once).
//!
//! ## Topics
//! - `shinobi.vcs.operations` : opérations VCS créées (→ Tensai consumer)
//! - `shinobi.tensai.analysis-complete` : analyse sémantique terminée (→ downstream)

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::operation::Operation;
use crate::entities::webhook::WebhookEvent;
use crate::errors::DomainError;

/// Résumé d'analyse sémantique publié sur le bus après traitement Tensai.
///
/// Contient uniquement les métriques — pas les chunks eux-mêmes (trop volumineux).
/// Les consommateurs downstream peuvent interroger l'API REST/gRPC pour les détails.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalysisCompleteSummary {
    /// ID de l'opération analysée.
    pub operation_id: Uuid,
    /// ID du dépôt auquel appartient l'opération (Phase 10A — isolation multi-tenant).
    pub repository_id: Uuid,
    /// Nombre de fichiers analysés (langages supportés).
    pub analyzed_files: usize,
    /// Nombre de fichiers ignorés (langages non supportés).
    pub skipped_files: usize,
    /// Nombre total de chunks sémantiques extraits.
    pub total_chunks: usize,
    /// Nombre de chunks avec embedding vectoriel.
    pub embedded_count: usize,
    /// Durée de l'analyse en millisecondes.
    pub duration_ms: u64,
}

/// Contrat de publication d'événements métier.
///
/// Conçu pour le pattern événementiel : chaque mutation métier
/// significative est propagée sur le bus pour les consommateurs
/// downstream (CI/CD, monitoring, IA, webhooks).
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publie un événement "opération créée" sur le bus.
    ///
    /// Topic: `shinobi.vcs.operations`
    /// La clé du message est l'`operation.id` (partitionnement Kafka).
    async fn publish_operation_created(
        &self,
        operation: &Operation,
    ) -> Result<(), DomainError>;

    /// Publie un événement "analyse sémantique terminée" sur le bus.
    ///
    /// Topic: `shinobi.tensai.analysis-complete` (topic dédié, PAS le topic VCS
    /// pour éviter la boucle infinie du consumer Tensai).
    async fn publish_analysis_complete(
        &self,
        summary: &AnalysisCompleteSummary,
    ) -> Result<(), DomainError>;

    /// Publie un événement webhook sur le bus Chakra (Phase 34-V2).
    ///
    /// Topic: `shinobi.events.webhooks`
    /// Le ChakraConsumer en background dispatche vers les endpoints HTTP abonnés.
    /// Fire-and-forget : le Use Case ne bloque pas sur la livraison HTTP.
    ///
    /// Si le système Chakra est désactivé, cette méthode retourne `Ok(())`
    /// silencieusement (graceful degradation).
    async fn publish_webhook_event(
        &self,
        event: &WebhookEvent,
    ) -> Result<(), DomainError>;

    /// Publie un événement "pipeline requested" pour le Jutsu Runner (Phase 40).
    ///
    /// Topic: `shinobi.jutsu.pipeline`
    /// Déclenché par le hook post-push quand un `jutsu.yml` est détecté
    /// à la racine du dépôt.
    ///
    /// Le message contient les informations nécessaires au JutsuConsumer
    /// pour lancer l'exécution du pipeline :
    /// - `repository_id` : UUID du dépôt
    /// - `commit_id` : SHA du commit à builder
    /// - `trigger_event` : type de déclencheur (push, mr_created, tag)
    ///
    /// Si le système Jutsu est désactivé, cette méthode retourne `Ok(())`
    /// silencieusement (graceful degradation).
    async fn publish_pipeline_requested(
        &self,
        repository_id: Uuid,
        commit_id: &str,
        trigger_event: &str,
    ) -> Result<(), DomainError>;
}
