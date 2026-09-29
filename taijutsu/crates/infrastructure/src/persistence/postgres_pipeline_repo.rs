//! Adaptateur PostgreSQL — PipelineRepository (Phase 40 — Jutsu Runner) 🥷⚡
//!
//! Implémente le port `PipelineRepository` pour le stockage PostgreSQL.
//! Gère les pipelines CI/CD natifs et leurs stages.
//!
//! ## Pattern
//! - Statuts stockés en VARCHAR(20) avec CHECK constraint SQL
//! - Row mapper privé pour isoler la couche SQL de la couche domaine
//! - Pattern identique à `postgres_commit_status_repo.rs`

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use domain::entities::pipeline::{
    Pipeline, PipelineStage, PipelineStageStatus, PipelineStatus, TriggerEvent,
};
use domain::entities::kage_bunshin::{HealAttempt, HealStatus, PatchHunk};
use domain::errors::DomainError;
use domain::ports::pipeline_repository::PipelineRepository;

/// Adaptateur PostgreSQL pour les pipelines CI/CD natifs.
pub struct PostgresPipelineRepo {
    pool: PgPool,
}

impl PostgresPipelineRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PipelineRepository for PostgresPipelineRepo {
    async fn create(&self, pipeline: &Pipeline) -> Result<Pipeline, DomainError> {
        let row = sqlx::query_as::<_, PipelineRow>(
            r#"
            INSERT INTO pipelines (
                id, repository_id, commit_id, trigger_event, status,
                pipeline_name, started_at, finished_at, duration_ms,
                creator_id, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
            )
            RETURNING id, repository_id, commit_id, trigger_event, status,
                      pipeline_name, started_at, finished_at, duration_ms,
                      creator_id, created_at, updated_at
            "#,
        )
        .bind(pipeline.id)
        .bind(pipeline.repository_id)
        .bind(&pipeline.commit_id)
        .bind(pipeline.trigger_event.as_sql_str())
        .bind(pipeline.status.as_sql_str())
        .bind(&pipeline.pipeline_name)
        .bind(pipeline.started_at)
        .bind(pipeline.finished_at)
        .bind(pipeline.duration_ms)
        .bind(pipeline.creator_id)
        .bind(pipeline.created_at)
        .bind(pipeline.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline create: {e}")))?;

        Ok(row.into_domain())
    }

    async fn update_status(
        &self,
        id: &Uuid,
        status: PipelineStatus,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
        duration_ms: Option<i32>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE pipelines
            SET status = $2,
                started_at = COALESCE($3, started_at),
                finished_at = COALESCE($4, finished_at),
                duration_ms = COALESCE($5, duration_ms),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status.as_sql_str())
        .bind(started_at)
        .bind(finished_at)
        .bind(duration_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline update_status: {e}")))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Pipeline>, DomainError> {
        let row = sqlx::query_as::<_, PipelineRow>(
            r#"
            SELECT id, repository_id, commit_id, trigger_event, status,
                   pipeline_name, started_at, finished_at, duration_ms,
                   creator_id, created_at, updated_at
            FROM pipelines
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline find_by_id: {e}")))?;

        Ok(row.map(|r| r.into_domain()))
    }

    async fn list_by_repo(
        &self,
        repository_id: &Uuid,
        limit: i64,
    ) -> Result<Vec<Pipeline>, DomainError> {
        let rows = sqlx::query_as::<_, PipelineRow>(
            r#"
            SELECT id, repository_id, commit_id, trigger_event, status,
                   pipeline_name, started_at, finished_at, duration_ms,
                   creator_id, created_at, updated_at
            FROM pipelines
            WHERE repository_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(repository_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline list_by_repo: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    async fn create_stage(
        &self,
        stage: &PipelineStage,
    ) -> Result<PipelineStage, DomainError> {
        let row = sqlx::query_as::<_, PipelineStageRow>(
            r#"
            INSERT INTO pipeline_stages (
                id, pipeline_id, name, image, status, sort_order,
                started_at, finished_at, duration_ms, logs, exit_code,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
            )
            RETURNING id, pipeline_id, name, image, status, sort_order,
                      started_at, finished_at, duration_ms, logs, exit_code,
                      created_at, updated_at
            "#,
        )
        .bind(stage.id)
        .bind(stage.pipeline_id)
        .bind(&stage.name)
        .bind(&stage.image)
        .bind(stage.status.as_sql_str())
        .bind(stage.sort_order)
        .bind(stage.started_at)
        .bind(stage.finished_at)
        .bind(stage.duration_ms)
        .bind(&stage.logs)
        .bind(stage.exit_code)
        .bind(stage.created_at)
        .bind(stage.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline_stage create: {e}")))?;

        Ok(row.into_domain())
    }

    async fn update_stage_status(
        &self,
        id: &Uuid,
        status: PipelineStageStatus,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
        duration_ms: Option<i32>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE pipeline_stages
            SET status = $2,
                started_at = COALESCE($3, started_at),
                finished_at = COALESCE($4, finished_at),
                duration_ms = COALESCE($5, duration_ms),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status.as_sql_str())
        .bind(started_at)
        .bind(finished_at)
        .bind(duration_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline_stage update_status: {e}")))?;

        Ok(())
    }

    async fn update_stage_logs(
        &self,
        id: &Uuid,
        logs: &str,
        exit_code: i16,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE pipeline_stages
            SET logs = $2,
                exit_code = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(logs)
        .bind(exit_code)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline_stage update_stage_logs: {e}")))?;

        Ok(())
    }

    async fn list_stages(
        &self,
        pipeline_id: &Uuid,
    ) -> Result<Vec<PipelineStage>, DomainError> {
        let rows = sqlx::query_as::<_, PipelineStageRow>(
            r#"
            SELECT id, pipeline_id, name, image, status, sort_order,
                   started_at, finished_at, duration_ms, logs, exit_code,
                   created_at, updated_at
            FROM pipeline_stages
            WHERE pipeline_id = $1
            ORDER BY sort_order ASC
            "#,
        )
        .bind(pipeline_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("pipeline_stage list_stages: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    // ── Phase 41 — Kage Bunshin (影分身) ─────────────────────────

    async fn create_heal_attempt(
        &self,
        attempt: &HealAttempt,
    ) -> Result<HealAttempt, DomainError> {
        let hunks_json = serde_json::to_value(&attempt.hunks)
            .map_err(|e| DomainError::Internal(format!("hunks serialize: {e}")))?;

        let row = sqlx::query_as::<_, HealAttemptRow>(
            r#"
            INSERT INTO heal_attempts (
                id, pipeline_id, stage_name, diagnosis, patch_summary,
                hunks, status, shadow_branch, mr_id, retry_logs,
                retry_exit_code, llm_model, llm_duration_ms, confidence,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16
            )
            RETURNING id, pipeline_id, stage_name, diagnosis, patch_summary,
                      hunks, status, shadow_branch, mr_id, retry_logs,
                      retry_exit_code, llm_model, llm_duration_ms, confidence,
                      created_at, updated_at
            "#,
        )
        .bind(attempt.id)
        .bind(attempt.pipeline_id)
        .bind(&attempt.stage_name)
        .bind(&attempt.diagnosis)
        .bind(&attempt.patch_summary)
        .bind(&hunks_json)
        .bind(attempt.status.as_sql_str())
        .bind(&attempt.shadow_branch)
        .bind(attempt.mr_id)
        .bind(&attempt.retry_logs)
        .bind(attempt.retry_exit_code)
        .bind(&attempt.llm_model)
        .bind(attempt.llm_duration_ms)
        .bind(attempt.confidence)
        .bind(attempt.created_at)
        .bind(attempt.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("heal_attempt create: {e}")))?;

        Ok(row.into_domain())
    }

    async fn update_heal_attempt(
        &self,
        id: &Uuid,
        status: HealStatus,
        shadow_branch: Option<&str>,
        mr_id: Option<Uuid>,
        retry_logs: Option<&str>,
        retry_exit_code: Option<i16>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE heal_attempts
            SET status = $2,
                shadow_branch = COALESCE($3, shadow_branch),
                mr_id = COALESCE($4, mr_id),
                retry_logs = COALESCE($5, retry_logs),
                retry_exit_code = COALESCE($6, retry_exit_code),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status.as_sql_str())
        .bind(shadow_branch)
        .bind(mr_id)
        .bind(retry_logs)
        .bind(retry_exit_code)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("heal_attempt update: {e}")))?;

        Ok(())
    }

    async fn list_heal_attempts(
        &self,
        pipeline_id: &Uuid,
    ) -> Result<Vec<HealAttempt>, DomainError> {
        let rows = sqlx::query_as::<_, HealAttemptRow>(
            r#"
            SELECT id, pipeline_id, stage_name, diagnosis, patch_summary,
                   hunks, status, shadow_branch, mr_id, retry_logs,
                   retry_exit_code, llm_model, llm_duration_ms, confidence,
                   created_at, updated_at
            FROM heal_attempts
            WHERE pipeline_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(pipeline_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("heal_attempt list: {e}")))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    async fn update_heal_attempt_mr(
        &self,
        pipeline_id: &Uuid,
        stage_name: &str,
        mr_id: &Uuid,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            UPDATE heal_attempts
            SET mr_id = $1,
                updated_at = now()
            WHERE id = (
                SELECT id FROM heal_attempts
                WHERE pipeline_id = $2 AND stage_name = $3
                ORDER BY created_at DESC
                LIMIT 1
            )
            "#,
        )
        .bind(mr_id)
        .bind(pipeline_id)
        .bind(stage_name)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Persistence(format!("heal_attempt update mr_id: {e}")))?;

        Ok(())
    }
}

// ── Row Mappers ───────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct PipelineRow {
    id: Uuid,
    repository_id: Uuid,
    commit_id: String,
    trigger_event: String,
    status: String,
    pipeline_name: Option<String>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    duration_ms: Option<i32>,
    creator_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PipelineRow {
    fn into_domain(self) -> Pipeline {
        Pipeline {
            id: self.id,
            repository_id: self.repository_id,
            commit_id: self.commit_id,
            trigger_event: TriggerEvent::from_sql(&self.trigger_event)
                .unwrap_or(TriggerEvent::Manual),
            status: PipelineStatus::from_sql(&self.status)
                .unwrap_or(PipelineStatus::Queued),
            pipeline_name: self.pipeline_name,
            started_at: self.started_at,
            finished_at: self.finished_at,
            duration_ms: self.duration_ms,
            creator_id: self.creator_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PipelineStageRow {
    id: Uuid,
    pipeline_id: Uuid,
    name: String,
    image: String,
    status: String,
    sort_order: i16,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    duration_ms: Option<i32>,
    logs: Option<String>,
    exit_code: Option<i16>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PipelineStageRow {
    fn into_domain(self) -> PipelineStage {
        PipelineStage {
            id: self.id,
            pipeline_id: self.pipeline_id,
            name: self.name,
            image: self.image,
            status: PipelineStageStatus::from_sql(&self.status)
                .unwrap_or(PipelineStageStatus::Pending),
            sort_order: self.sort_order,
            started_at: self.started_at,
            finished_at: self.finished_at,
            duration_ms: self.duration_ms,
            logs: self.logs,
            exit_code: self.exit_code,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

// ── Phase 41 — Kage Bunshin Row Mapper ────────────────────────────────

#[derive(sqlx::FromRow)]
struct HealAttemptRow {
    id: Uuid,
    pipeline_id: Uuid,
    stage_name: String,
    diagnosis: String,
    patch_summary: Option<String>,
    hunks: serde_json::Value,
    status: String,
    shadow_branch: Option<String>,
    mr_id: Option<Uuid>,
    retry_logs: Option<String>,
    retry_exit_code: Option<i16>,
    llm_model: Option<String>,
    llm_duration_ms: Option<i32>,
    confidence: Option<f32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HealAttemptRow {
    fn into_domain(self) -> HealAttempt {
        let hunks: Vec<PatchHunk> = serde_json::from_value(self.hunks)
            .unwrap_or_default();

        HealAttempt {
            id: self.id,
            pipeline_id: self.pipeline_id,
            stage_name: self.stage_name,
            diagnosis: self.diagnosis,
            patch_summary: self.patch_summary,
            hunks,
            status: HealStatus::from_sql_str(&self.status)
                .unwrap_or(HealStatus::Pending),
            shadow_branch: self.shadow_branch,
            mr_id: self.mr_id,
            retry_logs: self.retry_logs,
            retry_exit_code: self.retry_exit_code,
            llm_model: self.llm_model,
            llm_duration_ms: self.llm_duration_ms,
            confidence: self.confidence,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

