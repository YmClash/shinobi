//! Support de test partagé — Phase 42-pre-Fix (Kage Bunshin V2) 🧪
//!
//! Doubles en mémoire pour tester `RunPipelineUseCase` et
//! `FinalizePipelineUseCase` sans PostgreSQL, Docker ni Kafka.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use domain::entities::kage_bunshin::{HealAttempt, HealStatus};
use domain::entities::operation::Operation;
use domain::entities::pipeline::{
    Pipeline, PipelineStage, PipelineStageStatus, PipelineStatus, TriggerEvent,
};
use domain::entities::webhook::WebhookEvent;
use domain::errors::DomainError;
use domain::ports::container_runner::{ContainerRunResult, ContainerRunner};
use domain::ports::event_publisher::{AnalysisCompleteSummary, EventPublisher};
use domain::ports::pipeline_repository::PipelineRepository;

use super::finalize_pipeline::CommitStatusReporter;

// ── PipelineRepository en mémoire ────────────────────────────────────

#[derive(Default)]
pub struct InMemoryPipelineRepo {
    pub pipelines: Mutex<HashMap<Uuid, Pipeline>>,
    pub stages: Mutex<Vec<PipelineStage>>,
}

impl InMemoryPipelineRepo {
    pub fn stage_by_name(&self, name: &str) -> PipelineStage {
        self.stages
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .expect("stage introuvable")
    }

    pub fn pipeline(&self, id: &Uuid) -> Pipeline {
        self.pipelines.lock().unwrap().get(id).cloned().expect("pipeline introuvable")
    }

    /// Vieillit artificiellement un stage (simulation du temps pour le watchdog).
    pub fn age_stage(&self, id: &Uuid, secs: i64) {
        let mut stages = self.stages.lock().unwrap();
        if let Some(s) = stages.iter_mut().find(|s| s.id == *id) {
            s.updated_at = Utc::now() - chrono::Duration::seconds(secs);
        }
    }
}

#[async_trait]
impl PipelineRepository for InMemoryPipelineRepo {
    async fn create(&self, pipeline: &Pipeline) -> Result<Pipeline, DomainError> {
        self.pipelines.lock().unwrap().insert(pipeline.id, pipeline.clone());
        Ok(pipeline.clone())
    }

    async fn update_status(
        &self,
        id: &Uuid,
        status: PipelineStatus,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
        duration_ms: Option<i32>,
    ) -> Result<(), DomainError> {
        if let Some(p) = self.pipelines.lock().unwrap().get_mut(id) {
            p.status = status;
            p.started_at = started_at.or(p.started_at);
            p.finished_at = finished_at.or(p.finished_at);
            p.duration_ms = duration_ms.or(p.duration_ms);
        }
        Ok(())
    }

    async fn set_pipeline_name(&self, id: &Uuid, name: &str) -> Result<(), DomainError> {
        if let Some(p) = self.pipelines.lock().unwrap().get_mut(id) {
            if p.pipeline_name.is_none() {
                p.pipeline_name = Some(name.to_string());
            }
        }
        Ok(())
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Pipeline>, DomainError> {
        Ok(self.pipelines.lock().unwrap().get(id).cloned())
    }

    async fn list_by_repo(&self, _: &Uuid, _: i64) -> Result<Vec<Pipeline>, DomainError> {
        Ok(vec![])
    }

    async fn create_stage(&self, stage: &PipelineStage) -> Result<PipelineStage, DomainError> {
        self.stages.lock().unwrap().push(stage.clone());
        Ok(stage.clone())
    }

    async fn update_stage_status(
        &self,
        id: &Uuid,
        status: PipelineStageStatus,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
        duration_ms: Option<i32>,
    ) -> Result<(), DomainError> {
        if let Some(s) = self.stages.lock().unwrap().iter_mut().find(|s| s.id == *id) {
            s.status = status;
            s.started_at = started_at.or(s.started_at);
            s.finished_at = finished_at.or(s.finished_at);
            s.duration_ms = duration_ms.or(s.duration_ms);
            s.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn update_stage_logs(&self, id: &Uuid, logs: &str, exit_code: i16) -> Result<(), DomainError> {
        if let Some(s) = self.stages.lock().unwrap().iter_mut().find(|s| s.id == *id) {
            s.logs = Some(logs.to_string());
            s.exit_code = Some(exit_code);
            s.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn list_stages(&self, pipeline_id: &Uuid) -> Result<Vec<PipelineStage>, DomainError> {
        let mut v: Vec<_> = self
            .stages
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.pipeline_id == *pipeline_id)
            .cloned()
            .collect();
        v.sort_by_key(|s| s.sort_order);
        Ok(v)
    }

    async fn create_heal_attempt(&self, a: &HealAttempt) -> Result<HealAttempt, DomainError> {
        Ok(a.clone())
    }

    async fn update_heal_attempt(
        &self,
        _: &Uuid,
        _: HealStatus,
        _: Option<&str>,
        _: Option<Uuid>,
        _: Option<&str>,
        _: Option<i16>,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn list_heal_attempts(&self, _: &Uuid) -> Result<Vec<HealAttempt>, DomainError> {
        Ok(vec![])
    }

    async fn update_heal_attempt_mr(&self, _: &Uuid, _: &str, _: &Uuid) -> Result<(), DomainError> {
        Ok(())
    }

    async fn exists_for_commit(
        &self,
        repository_id: &Uuid,
        commit_id: &str,
        trigger_event: &TriggerEvent,
    ) -> Result<bool, DomainError> {
        Ok(self.pipelines.lock().unwrap().values().any(|p| {
            p.repository_id == *repository_id
                && p.commit_id == commit_id
                && p.trigger_event == *trigger_event
        }))
    }

    async fn transition_stage_status(
        &self,
        id: &Uuid,
        from: PipelineStageStatus,
        to: PipelineStageStatus,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<bool, DomainError> {
        let mut stages = self.stages.lock().unwrap();
        match stages.iter_mut().find(|s| s.id == *id) {
            Some(s) if s.status == from => {
                s.status = to;
                s.finished_at = finished_at.or(s.finished_at);
                s.updated_at = Utc::now();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn list_stale_healing_stages(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<PipelineStage>, DomainError> {
        Ok(self
            .stages
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.status == PipelineStageStatus::Healing && s.updated_at < cutoff)
            .cloned()
            .collect())
    }

    async fn complete_pipeline(
        &self,
        id: &Uuid,
        status: PipelineStatus,
        finished_at: DateTime<Utc>,
        duration_ms: Option<i32>,
    ) -> Result<bool, DomainError> {
        let mut pipelines = self.pipelines.lock().unwrap();
        match pipelines.get_mut(id) {
            Some(p) if matches!(p.status, PipelineStatus::Queued | PipelineStatus::Running) => {
                p.status = status;
                p.finished_at = Some(finished_at);
                p.duration_ms = duration_ms.or(p.duration_ms);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

// ── CommitStatusReporter factice ─────────────────────────────────────

#[derive(Default)]
pub struct RecordingReporter {
    /// (state, context, description)
    pub reports: Mutex<Vec<(String, String, String)>>,
}

impl RecordingReporter {
    pub fn states(&self) -> Vec<String> {
        self.reports.lock().unwrap().iter().map(|r| r.0.clone()).collect()
    }
}

#[async_trait]
impl CommitStatusReporter for RecordingReporter {
    async fn report(
        &self,
        _actor_id: Uuid,
        _repository_id: Uuid,
        _commit_id: &str,
        state: &str,
        context: &str,
        description: Option<String>,
    ) -> Result<(), DomainError> {
        self.reports.lock().unwrap().push((
            state.to_string(),
            context.to_string(),
            description.unwrap_or_default(),
        ));
        Ok(())
    }
}

// ── EventPublisher factice ───────────────────────────────────────────

#[derive(Default)]
pub struct RecordingPublisher {
    pub webhooks: Mutex<Vec<WebhookEvent>>,
    /// stage_name des demandes Kage Bunshin publiées.
    pub heals: Mutex<Vec<String>>,
}

#[async_trait]
impl EventPublisher for RecordingPublisher {
    async fn publish_operation_created(&self, _: &Operation) -> Result<(), DomainError> {
        Ok(())
    }

    async fn publish_analysis_complete(&self, _: &AnalysisCompleteSummary) -> Result<(), DomainError> {
        Ok(())
    }

    async fn publish_webhook_event(&self, event: &WebhookEvent) -> Result<(), DomainError> {
        self.webhooks.lock().unwrap().push(event.clone());
        Ok(())
    }

    async fn publish_pipeline_requested(
        &self,
        _: Uuid,
        _: &str,
        _: &str,
        _: Option<Uuid>,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn publish_kage_bunshin_requested(
        &self,
        _pipeline_id: Uuid,
        _stage_id: Uuid,
        stage_name: &str,
        _stage_image: &str,
        _stage_commands: &[String],
        _error_logs: &str,
        _repository_id: Uuid,
        _commit_id: &str,
    ) -> Result<(), DomainError> {
        self.heals.lock().unwrap().push(stage_name.to_string());
        Ok(())
    }
}

// ── ContainerRunner scripté ──────────────────────────────────────────

/// Runner factice : exit code par image (défaut 0) + trace des images lancées.
#[derive(Default)]
pub struct ScriptedRunner {
    pub exit_codes: HashMap<String, i64>,
    pub ran: Mutex<Vec<String>>,
}

#[async_trait]
impl ContainerRunner for ScriptedRunner {
    async fn run_stage(
        &self,
        image: &str,
        _commands: &[String],
        _workspace_path: &Path,
        _timeout: Duration,
        _pipeline_id: &str,
    ) -> Result<ContainerRunResult, DomainError> {
        self.ran.lock().unwrap().push(image.to_string());
        let exit_code = *self.exit_codes.get(image).unwrap_or(&0);
        Ok(ContainerRunResult {
            exit_code,
            logs: format!("logs {image} exit={exit_code}"),
            duration_ms: 1,
        })
    }
}
