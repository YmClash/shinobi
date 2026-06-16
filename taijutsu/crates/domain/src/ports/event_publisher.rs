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
use crate::errors::DomainError;

/// Résumé d'analyse sémantique publié sur le bus après traitement Tensai.
///
/// Contient uniquement les métriques — pas les chunks eux-mêmes (trop volumineux).
/// Les consommateurs downstream peuvent interroger l'API REST/gRPC pour les détails.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalysisCompleteSummary {
    /// ID de l'opération analysée.
    pub operation_id: Uuid,
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
/// downstream (CI/CD, monitoring, IA).
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
}
