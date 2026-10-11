//! Use Case : Finalisation d'un Pipeline — Phase 42-pre-Fix (Kage Bunshin V2) 🥷🏁
//!
//! **Point unique** de fin de vie d'un pipeline Jutsu. Trois chemins y
//! convergent et produisent exactement les mêmes effets de bord :
//!
//! ```text
//! RunPipelineUseCase      (fin normale)          ─┐
//! KageBunshinConsumer     (heal réussi / échoué)  ├─► FinalizePipelineUseCase
//! KageBunshinWatchdog     (heal expiré)          ─┘     ├─ complete_pipeline (CAS BDD)
//!                                                       ├─ commit status (pastille UI)
//!                                                       └─ webhook `pipeline_completed`
//! ```
//!
//! ## Garanties
//! - **Annonce unique** : `complete_pipeline` est un compare-and-set
//!   (`WHERE status IN ('queued','running')`) — seul le gagnant annonce.
//! - **Heal résolu une seule fois** : `transition_stage_status(Healing → X)`
//!   est atomique ; si le watchdog a expiré le stage, le consumer perd la
//!   course proprement (et inversement).
//! - **Makimono** : l'UI suit les pipelines par polling SWR sur la BDD —
//!   la mise à jour en base suffit à rafraîchir l'écran. Les intégrations
//!   externes reçoivent le webhook `pipeline_completed` via Chakra/Kafka.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::actor::SYSTEM_ACTOR_ID;
use domain::entities::pipeline::{
    compute_pipeline_status, Pipeline, PipelineStage, PipelineStageStatus, PipelineStatus,
};
use domain::entities::webhook::{WebhookEvent, WebhookEventType};
use domain::errors::DomainError;
use domain::ports::event_publisher::EventPublisher;
use domain::ports::pipeline_repository::PipelineRepository;

use super::manage_commit_statuses::ManageCommitStatusesUseCase;

// ── Abstraction du reporting de commit status ─────────────────────────

/// Reporter de statut de commit pour un dépôt connu par son UUID.
///
/// Implémenté par `ManageCommitStatusesUseCase` ; abstrait ici pour
/// rendre la finalisation testable sans `RepoRepository`/`ActorRepository`.
#[async_trait]
pub trait CommitStatusReporter: Send + Sync {
    async fn report(
        &self,
        actor_id: Uuid,
        repository_id: Uuid,
        commit_id: &str,
        state: &str,
        context: &str,
        description: Option<String>,
    ) -> Result<(), DomainError>;
}

#[async_trait]
impl CommitStatusReporter for ManageCommitStatusesUseCase {
    async fn report(
        &self,
        actor_id: Uuid,
        repository_id: Uuid,
        commit_id: &str,
        state: &str,
        context: &str,
        description: Option<String>,
    ) -> Result<(), DomainError> {
        self.report_for_repository(actor_id, repository_id, commit_id, state, context, description)
            .await
            .map(|_| ())
    }
}

// ── Types utilitaires ────────────────────────────────────────────────

/// Origine de la finalisation (tracée dans le webhook et les logs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizedBy {
    /// Fin normale dans `RunPipelineUseCase`.
    Runner,
    /// Fin d'un heal dans le `KageBunshinConsumer`.
    KageBunshin,
    /// Heal expiré, finalisé par le watchdog.
    Watchdog,
}

impl FinalizedBy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Runner => "runner",
            Self::KageBunshin => "kage_bunshin",
            Self::Watchdog => "watchdog",
        }
    }
}

/// État de commit status correspondant à un statut de pipeline.
///
/// `running`/`queued` → `pending` (Kage Bunshin en cours : PAS `error`).
pub fn commit_state_for(status: &PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Success => "success",
        PipelineStatus::Failure => "failure",
        PipelineStatus::Queued | PipelineStatus::Running => "pending",
        PipelineStatus::Error | PipelineStatus::Cancelled => "error",
    }
}

/// Context du commit status : `jutsu/<pipeline_name>`.
pub fn pipeline_context(pipeline: &Pipeline) -> String {
    format!("jutsu/{}", pipeline.pipeline_name.as_deref().unwrap_or("pipeline"))
}

// ── Use Case ─────────────────────────────────────────────────────────

/// Finalisation unifiée des pipelines (runner, Kage Bunshin, watchdog).
pub struct FinalizePipelineUseCase {
    pipeline_repo: Arc<dyn PipelineRepository>,
    reporter: Arc<dyn CommitStatusReporter>,
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl FinalizePipelineUseCase {
    pub fn new(
        pipeline_repo: Arc<dyn PipelineRepository>,
        reporter: Arc<dyn CommitStatusReporter>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self {
            pipeline_repo,
            reporter,
            event_publisher,
        }
    }

    /// Reporte un commit status pour ce pipeline (non-fatal).
    pub async fn report_commit_status(&self, pipeline: &Pipeline, state: &str, description: String) {
        if let Err(e) = self
            .reporter
            .report(
                pipeline.creator_id.unwrap_or(SYSTEM_ACTOR_ID),
                pipeline.repository_id,
                &pipeline.commit_id,
                state,
                &pipeline_context(pipeline),
                Some(description),
            )
            .await
        {
            warn!(pipeline_id = %pipeline.id, error = %e, "⚠️ Commit status report failed (non-fatal)");
        }
    }

    /// Finalise atomiquement le pipeline puis annonce sa fin.
    ///
    /// Retourne `true` si cet appel a effectué la finalisation (et donc
    /// l'annonce), `false` si un autre chemin l'avait déjà fait.
    pub async fn complete(
        &self,
        pipeline_id: &Uuid,
        status: PipelineStatus,
        by: FinalizedBy,
    ) -> Result<bool, DomainError> {
        let pipeline = self
            .pipeline_repo
            .find_by_id(pipeline_id)
            .await?
            .ok_or(DomainError::NotFound {
                entity_type: "Pipeline",
                id: *pipeline_id,
            })?;

        let finished_at = Utc::now();
        let duration_ms = pipeline
            .started_at
            .map(|s| (finished_at - s).num_milliseconds() as i32);

        let won = self
            .pipeline_repo
            .complete_pipeline(pipeline_id, status.clone(), finished_at, duration_ms)
            .await?;

        if !won {
            info!(
                pipeline_id = %pipeline_id,
                by = by.as_str(),
                "🏁 Pipeline déjà finalisé par un autre chemin — annonce ignorée"
            );
            return Ok(false);
        }

        let stages = self.pipeline_repo.list_stages(pipeline_id).await?;
        let mut finalized = pipeline;
        finalized.status = status.clone();
        finalized.finished_at = Some(finished_at);
        finalized.duration_ms = duration_ms.or(finalized.duration_ms);

        self.announce(&finalized, &stages, by).await;
        Ok(true)
    }

    /// Recalcule le statut à partir des stages et finalise si tous sont terminaux.
    ///
    /// Retourne `Some(status)` si le pipeline vient d'être finalisé par cet appel.
    pub async fn finalize_from_stages(
        &self,
        pipeline_id: &Uuid,
        by: FinalizedBy,
    ) -> Result<Option<PipelineStatus>, DomainError> {
        let stages = self.pipeline_repo.list_stages(pipeline_id).await?;
        let Some(status) = compute_pipeline_status(&stages) else {
            info!(pipeline_id = %pipeline_id, "🥷 Pipeline pas encore finalisable — stages en cours");
            return Ok(None);
        };

        if self.complete(pipeline_id, status.clone(), by).await? {
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }

    /// Revendique un stage `healing` (garde d'idempotence + heartbeat).
    ///
    /// Transition atomique `Healing → Healing` qui rafraîchit `updated_at` :
    /// - `false` → le stage n'est plus en guérison (replay Kafka, heal
    ///   déjà traité ou expiré) : le consumer doit ignorer la demande ;
    /// - `true` → le compteur du watchdog repart de zéro.
    pub async fn claim_heal(&self, stage_id: &Uuid) -> Result<bool, DomainError> {
        self.pipeline_repo
            .transition_stage_status(
                stage_id,
                PipelineStageStatus::Healing,
                PipelineStageStatus::Healing,
                None,
            )
            .await
    }

    /// Résout un heal (`Healing → Healed | Failure`) puis tente la finalisation.
    ///
    /// Retourne `false` si le stage avait déjà quitté l'état `healing`
    /// (ex : expiré par le watchdog) — rien n'est modifié dans ce cas.
    pub async fn resolve_heal(
        &self,
        pipeline_id: &Uuid,
        stage_id: &Uuid,
        outcome: PipelineStageStatus,
        note: Option<&str>,
        by: FinalizedBy,
    ) -> Result<bool, DomainError> {
        let applied = self
            .pipeline_repo
            .transition_stage_status(
                stage_id,
                PipelineStageStatus::Healing,
                outcome.clone(),
                Some(Utc::now()),
            )
            .await?;

        if !applied {
            info!(
                stage_id = %stage_id,
                outcome = %outcome,
                by = by.as_str(),
                "🥷 Heal déjà résolu par un autre chemin — transition ignorée"
            );
            return Ok(false);
        }

        if let Some(note) = note {
            self.append_stage_note(pipeline_id, stage_id, note).await;
        }

        self.finalize_from_stages(pipeline_id, by).await?;
        Ok(true)
    }

    /// Watchdog : expire les heals sans nouvelles depuis `timeout`.
    ///
    /// Retourne le nombre de stages expirés par cet appel.
    pub async fn expire_stale_heals(&self, timeout: Duration) -> Result<usize, DomainError> {
        let timeout_chrono =
            chrono::Duration::from_std(timeout).unwrap_or_else(|_| chrono::Duration::seconds(600));
        let cutoff = Utc::now() - timeout_chrono;
        let stale = self.pipeline_repo.list_stale_healing_stages(cutoff).await?;

        let mut expired = 0;
        for stage in stale {
            let note = format!(
                "[kage-bunshin] ⏰ Heal expiré après {}s sans réponse — stage marqué en échec par le watchdog",
                timeout.as_secs()
            );
            match self
                .resolve_heal(
                    &stage.pipeline_id,
                    &stage.id,
                    PipelineStageStatus::Failure,
                    Some(&note),
                    FinalizedBy::Watchdog,
                )
                .await
            {
                Ok(true) => {
                    expired += 1;
                    warn!(
                        pipeline_id = %stage.pipeline_id,
                        stage = %stage.name,
                        timeout_secs = timeout.as_secs(),
                        "⏰ Kage Bunshin watchdog — heal expiré → failure"
                    );
                }
                Ok(false) => {}
                Err(e) => {
                    warn!(stage = %stage.name, error = %e, "⚠️ Watchdog — expiration échouée");
                }
            }
        }

        Ok(expired)
    }

    // ── Internes ─────────────────────────────────────────────────────

    /// Annonce la fin : commit status final + webhook `pipeline_completed`.
    async fn announce(&self, pipeline: &Pipeline, stages: &[PipelineStage], by: FinalizedBy) {
        let succeeded = stages
            .iter()
            .filter(|s| s.status == PipelineStageStatus::Success)
            .count();
        let healed: Vec<&str> = stages
            .iter()
            .filter(|s| s.status == PipelineStageStatus::Healed)
            .map(|s| s.name.as_str())
            .collect();

        let suffix = if !healed.is_empty() {
            format!(" · correctif Kage Bunshin proposé en MR ({})", healed.join(", "))
        } else if by == FinalizedBy::Watchdog {
            " · heal expiré (watchdog)".to_string()
        } else {
            String::new()
        };

        let description = format!(
            "Pipeline '{}' {} — {}/{} stages réussis{}",
            pipeline.pipeline_name.as_deref().unwrap_or("pipeline"),
            pipeline.status,
            succeeded,
            stages.len(),
            suffix,
        );

        self.report_commit_status(pipeline, commit_state_for(&pipeline.status), description)
            .await;

        if let Some(publisher) = &self.event_publisher {
            let payload = serde_json::json!({
                "pipeline": {
                    "id": pipeline.id,
                    "name": pipeline.pipeline_name,
                    "status": pipeline.status.as_sql_str(),
                    "commit_id": pipeline.commit_id,
                    "trigger_event": pipeline.trigger_event.as_sql_str(),
                    "started_at": pipeline.started_at,
                    "finished_at": pipeline.finished_at,
                    "duration_ms": pipeline.duration_ms,
                },
                "stages": stages.iter().map(|s| serde_json::json!({
                    "name": s.name,
                    "status": s.status.as_sql_str(),
                    "exit_code": s.exit_code,
                    "duration_ms": s.duration_ms,
                })).collect::<Vec<_>>(),
                "kage_bunshin": { "healed_stages": healed },
                "finalized_by": by.as_str(),
            });

            let event = WebhookEvent::new(
                WebhookEventType::PipelineCompleted,
                pipeline.repository_id,
                pipeline.creator_id.unwrap_or(SYSTEM_ACTOR_ID),
                payload,
            );

            if let Err(e) = publisher.publish_webhook_event(&event).await {
                warn!(pipeline_id = %pipeline.id, error = %e, "⚠️ Webhook pipeline_completed non publié (non-fatal)");
            }
        }

        info!(
            pipeline_id = %pipeline.id,
            status = %pipeline.status,
            by = by.as_str(),
            "🏁 Pipeline finalisé et annoncé (commit status + webhook)"
        );
    }

    /// Ajoute une note en fin de logs du stage (diagnostic visible dans l'UI).
    async fn append_stage_note(&self, pipeline_id: &Uuid, stage_id: &Uuid, note: &str) {
        let stage = match self.pipeline_repo.list_stages(pipeline_id).await {
            Ok(stages) => stages.into_iter().find(|s| s.id == *stage_id),
            Err(e) => {
                warn!(error = %e, "⚠️ Impossible de relire le stage pour annoter les logs");
                return;
            }
        };
        let Some(stage) = stage else { return };

        let logs = match stage.logs.as_deref() {
            Some(existing) if !existing.is_empty() => format!("{existing}\n\n{note}"),
            _ => note.to_string(),
        };

        if let Err(e) = self
            .pipeline_repo
            .update_stage_logs(stage_id, &logs, stage.exit_code.unwrap_or(-1))
            .await
        {
            warn!(error = %e, "⚠️ Impossible d'annoter les logs du stage");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::use_cases::pipeline_test_support::{
        InMemoryPipelineRepo, RecordingPublisher, RecordingReporter,
    };
    use domain::entities::pipeline::TriggerEvent;

    struct Fixture {
        repo: Arc<InMemoryPipelineRepo>,
        reporter: Arc<RecordingReporter>,
        publisher: Arc<RecordingPublisher>,
        uc: FinalizePipelineUseCase,
        pipeline_id: Uuid,
    }

    /// Pipeline running avec 2 stages : A (healing) puis B (skipped).
    async fn fixture() -> Fixture {
        let repo = Arc::new(InMemoryPipelineRepo::default());
        let reporter = Arc::new(RecordingReporter::default());
        let publisher = Arc::new(RecordingPublisher::default());

        let mut p = Pipeline::new(Uuid::new_v4(), "abc123".into(), TriggerEvent::Push, Some("ci".into()), None);
        p.status = PipelineStatus::Running;
        p.started_at = Some(Utc::now());
        repo.create(&p).await.unwrap();

        let mut a = PipelineStage::new(p.id, "A".into(), "img".into(), 0);
        a.status = PipelineStageStatus::Healing;
        a.logs = Some("error[E0277]".into());
        a.exit_code = Some(101);
        let mut b = PipelineStage::new(p.id, "B".into(), "img".into(), 1);
        b.status = PipelineStageStatus::Skipped;
        repo.create_stage(&a).await.unwrap();
        repo.create_stage(&b).await.unwrap();

        let uc = FinalizePipelineUseCase::new(
            repo.clone(),
            reporter.clone(),
            Some(publisher.clone() as Arc<dyn EventPublisher>),
        );
        Fixture { repo, reporter, publisher, uc, pipeline_id: p.id }
    }

    #[tokio::test]
    async fn healed_heal_finalizes_as_failure_and_announces_once() {
        let f = fixture().await;
        let a = f.repo.stage_by_name("A");

        let applied = f
            .uc
            .resolve_heal(&f.pipeline_id, &a.id, PipelineStageStatus::Healed, None, FinalizedBy::KageBunshin)
            .await
            .unwrap();
        assert!(applied);

        assert_eq!(f.repo.pipeline(&f.pipeline_id).status, PipelineStatus::Failure);
        assert_eq!(f.reporter.states(), vec!["failure"]);
        let webhooks = f.publisher.webhooks.lock().unwrap();
        assert_eq!(webhooks.len(), 1);
        assert_eq!(webhooks[0].event_type, WebhookEventType::PipelineCompleted);
        assert_eq!(webhooks[0].payload["finalized_by"], "kage_bunshin");
        assert_eq!(webhooks[0].payload["kage_bunshin"]["healed_stages"][0], "A");
    }

    #[tokio::test]
    async fn watchdog_expires_stale_heal_and_announces() {
        let f = fixture().await;
        let a = f.repo.stage_by_name("A");
        f.repo.age_stage(&a.id, 700);

        let expired = f.uc.expire_stale_heals(Duration::from_secs(600)).await.unwrap();
        assert_eq!(expired, 1);

        let a = f.repo.stage_by_name("A");
        assert_eq!(a.status, PipelineStageStatus::Failure);
        assert!(a.logs.unwrap().contains("Heal expiré après 600s"));
        assert_eq!(f.repo.pipeline(&f.pipeline_id).status, PipelineStatus::Failure);
        assert_eq!(f.reporter.states(), vec!["failure"]);
        let webhooks = f.publisher.webhooks.lock().unwrap();
        assert_eq!(webhooks.len(), 1);
        assert_eq!(webhooks[0].payload["finalized_by"], "watchdog");
    }

    #[tokio::test]
    async fn watchdog_ignores_recent_heal() {
        let f = fixture().await;
        let expired = f.uc.expire_stale_heals(Duration::from_secs(600)).await.unwrap();
        assert_eq!(expired, 0);
        assert_eq!(f.repo.stage_by_name("A").status, PipelineStageStatus::Healing);
        assert!(f.publisher.webhooks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn late_heal_after_watchdog_is_ignored() {
        let f = fixture().await;
        let a = f.repo.stage_by_name("A");
        f.repo.age_stage(&a.id, 700);
        f.uc.expire_stale_heals(Duration::from_secs(600)).await.unwrap();

        // Le consumer termine après l'expiration : il perd la course.
        assert!(!f.uc.claim_heal(&a.id).await.unwrap());
        let applied = f
            .uc
            .resolve_heal(&f.pipeline_id, &a.id, PipelineStageStatus::Healed, None, FinalizedBy::KageBunshin)
            .await
            .unwrap();
        assert!(!applied);
        assert_eq!(f.repo.stage_by_name("A").status, PipelineStageStatus::Failure);
        // Toujours une seule annonce.
        assert_eq!(f.publisher.webhooks.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn claim_refreshes_heartbeat() {
        let f = fixture().await;
        let a = f.repo.stage_by_name("A");
        f.repo.age_stage(&a.id, 700);
        assert!(f.uc.claim_heal(&a.id).await.unwrap());
        // Après le claim, le stage n'est plus considéré comme expiré.
        assert_eq!(f.uc.expire_stale_heals(Duration::from_secs(600)).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn complete_is_announced_only_once() {
        let f = fixture().await;
        assert!(f.uc.complete(&f.pipeline_id, PipelineStatus::Failure, FinalizedBy::Runner).await.unwrap());
        assert!(!f.uc.complete(&f.pipeline_id, PipelineStatus::Failure, FinalizedBy::Watchdog).await.unwrap());
        assert_eq!(f.publisher.webhooks.lock().unwrap().len(), 1);
        assert_eq!(f.reporter.states().len(), 1);
    }

    #[test]
    fn running_maps_to_pending_commit_state() {
        assert_eq!(commit_state_for(&PipelineStatus::Running), "pending");
        assert_eq!(commit_state_for(&PipelineStatus::Failure), "failure");
        assert_eq!(commit_state_for(&PipelineStatus::Error), "error");
    }
}
