//! Port: PipelineRepository — Phase 40 (Jutsu Runner Natif) 🥷⚡
//!
//! Contrat d'accès aux pipelines CI/CD natifs et à leurs stages.
//! L'adaptateur concret dans infrastructure/ implémente le stockage PostgreSQL.
//!
//! ## Opérations principales
//! - **Pipeline CRUD** : Créer, mettre à jour le statut, trouver, lister
//! - **Stage CRUD** : Créer, mettre à jour le statut/logs, lister par pipeline

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::pipeline::{
    Pipeline, PipelineStage, PipelineStageStatus, PipelineStatus,
};
use crate::errors::DomainError;

/// Contrat d'accès aux pipelines CI/CD natifs.
#[async_trait]
pub trait PipelineRepository: Send + Sync {
    // ── Pipeline ────────────────────────────────────────────────

    /// Crée un nouveau pipeline en base de données.
    ///
    /// Le pipeline est créé en status `queued` — il sera mis à jour
    /// vers `running` quand le premier stage démarre.
    async fn create(&self, pipeline: &Pipeline) -> Result<Pipeline, DomainError>;

    /// Met à jour le statut d'un pipeline.
    ///
    /// Utilisé pour les transitions : queued→running, running→success/failure/error.
    /// Les timestamps sont mis à jour en conséquence.
    async fn update_status(
        &self,
        id: &Uuid,
        status: PipelineStatus,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
        duration_ms: Option<i32>,
    ) -> Result<(), DomainError>;

    /// Retrouve un pipeline par son identifiant.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Pipeline>, DomainError>;

    /// Liste les pipelines d'un dépôt, ordonnés par date de création DESC.
    ///
    /// `limit` : nombre maximum de résultats (pagination future).
    async fn list_by_repo(
        &self,
        repository_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<Pipeline>, DomainError>;

    // ── Pipeline Stages ─────────────────────────────────────────

    /// Crée un nouveau stage dans un pipeline.
    ///
    /// Le stage est créé en status `pending` — il sera mis à jour
    /// vers `running` quand le container Docker démarre.
    async fn create_stage(
        &self,
        stage: &PipelineStage,
    ) -> Result<PipelineStage, DomainError>;

    /// Met à jour le statut d'un stage.
    ///
    /// Transitions possibles :
    /// - pending → running (container démarré)
    /// - running → success/failure/error (container terminé)
    /// - pending → skipped (dépendance en échec)
    async fn update_stage_status(
        &self,
        id: &Uuid,
        status: PipelineStageStatus,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
        duration_ms: Option<i32>,
    ) -> Result<(), DomainError>;

    /// Met à jour les logs et le code de sortie d'un stage.
    ///
    /// Appelé à la fin de l'exécution du container Docker.
    /// Les logs contiennent stdout + stderr accumulés.
    async fn update_stage_logs(
        &self,
        id: &Uuid,
        logs: &str,
        exit_code: i16,
    ) -> Result<(), DomainError>;

    /// Liste les stages d'un pipeline, ordonnés par `sort_order ASC`.
    async fn list_stages(
        &self,
        pipeline_id: &Uuid,
    ) -> Result<Vec<PipelineStage>, DomainError>;
}
