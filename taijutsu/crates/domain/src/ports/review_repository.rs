//! Port: ReviewRepository — Contrat de persistence des code reviews IA.
//!
//! Ce trait abstrait le stockage des reviews produites par les agents actifs
//! (Oracle Reviewer). L'adaptateur concret dans infrastructure/ implémente
//! la persistence réelle via PostgreSQL.
//!
//! ## Phase 9 — L'Oracle Reviewer
//! L'agent Oracle écoute le topic `shinobi.tensai.analysis-complete`,
//! récupère le diff du commit, et utilise un LLM local (Ollama) pour
//! produire une code review automatique. Le résultat est persisté ici.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::errors::DomainError;

/// Review d'une opération VCS produite par un agent IA.
///
/// Chaque review contient le verdict du LLM local (Ollama) sur la qualité
/// du code modifié dans un commit. Le contenu est formaté en Markdown.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperationReview {
    /// Identifiant unique de la review.
    pub id: Uuid,
    /// ID de l'opération VCS reviewée.
    pub operation_id: Uuid,
    /// Identifiant de l'agent reviewer (ex: "oracle").
    pub reviewer: String,
    /// Nom du modèle LLM utilisé (ex: "granite3.1-dense:2b").
    pub model: String,
    /// Résumé court de la review (première ligne).
    pub summary: String,
    /// Review complète formatée en Markdown.
    pub content: String,
    /// Score de qualité optionnel (0.0 à 1.0).
    pub score: Option<f32>,
    /// Temps de génération LLM en millisecondes.
    pub duration_ms: u64,
    /// Date de création de la review.
    pub created_at: DateTime<Utc>,
}

/// Point de score pour la sparkline (Phase 9.2 — Électrocardiogramme).
///
/// Projection légère de `OperationReview` contenant uniquement les données
/// nécessaires au graphique de tendance (pas de contenu Markdown).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScorePoint {
    /// ID de l'opération VCS associée.
    pub operation_id: Uuid,
    /// Score de qualité (0.0 à 1.0).
    pub score: f32,
    /// Date de la review.
    pub created_at: DateTime<Utc>,
}

/// Contrat de persistence des code reviews IA.
///
/// Conçu pour le pattern agent actif : les reviews sont produites
/// de manière asynchrone par l'Oracle et persistées pour consultation
/// ultérieure via l'API REST ou le frontend Makimono.
#[async_trait]
pub trait ReviewRepository: Send + Sync {
    /// Sauvegarde une review dans le stockage persistant.
    async fn save_review(&self, review: &OperationReview) -> Result<(), DomainError>;

    /// Retrouve toutes les reviews d'une opération, triées par date décroissante.
    async fn find_by_operation(
        &self,
        operation_id: &Uuid,
    ) -> Result<Vec<OperationReview>, DomainError>;

    /// Supprime toutes les reviews d'une opération (idempotence).
    ///
    /// Retourne le nombre de reviews supprimées.
    async fn delete_by_operation(&self, operation_id: &Uuid) -> Result<u64, DomainError>;

    /// Récupère les N derniers scores (non-null) pour la sparkline.
    ///
    /// Filtre les reviews avec `score IS NOT NULL` — seules les reviews
    /// Phase 9.1+ ont un score déterministe.
    async fn find_recent_scores(&self, limit: usize) -> Result<Vec<ScorePoint>, DomainError>;
}

