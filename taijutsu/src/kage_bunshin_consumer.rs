//! Consumer Kafka — KageBunshinConsumer (Phase 41 — Auto-Healing) 🥷⚡
//!
//! Écoute le topic dédié `shinobi.jutsu.kage-bunshin` et exécute les
//! tentatives de guérison automatique de manière asynchrone.
//!
//! ## Vegapunk Tweak #10 — File Dédiée
//! Ce consumer est **séparé** du `JutsuConsumer` pour éviter le
//! "Kafka Starvation" : l'inférence LLM (30-60s) ne bloque plus
//! les workers pipeline normaux.
//!
//! ## Architecture
//! ```text
//! RunPipelineUseCase (stage failure + kage_bunshin: true)
//!   → publish_kage_bunshin_requested()
//!     → Kafka topic `shinobi.jutsu.kage-bunshin`
//!       → KageBunshinConsumer (ce fichier)
//!         → checkout code → KageBunshinUseCase::attempt_heal()
//!           → Si ✅ → CreateMrUseCase (auteur: Sensei service account)
//!           → Nettoyage: remove_dir_all (Tweak #9)
//! ```
//!
//! ## Pattern
//! - Consumer Kafka rdkafka (`StreamConsumer`)
//! - 1 worker par défaut (inférence LLM séquentielle)
//! - Workspace éphémère via `tempfile::TempDir`
//! - Graceful shutdown via `CancellationToken`

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Message;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use application::use_cases::kage_bunshin::KageBunshinUseCase;
use domain::entities::actor::{Actor, SENSEI_ACTOR_HANDLE, SYSTEM_ACTOR_ID};
use domain::entities::kage_bunshin::HealResult;
use domain::entities::merge_request::{MergeRequest, MrEvent, MrEventType};
use domain::ports::actor_repository::ActorRepository;
use domain::ports::mr_repository::MrRepository;
use domain::ports::pipeline_repository::PipelineRepository;
use domain::ports::vcs_engine::VcsEngine;
use domain::ports::repo_repository::RepoRepository;
use domain::entities::pipeline::{PipelineStageStatus, PipelineStatus};

/// Message reçu depuis Kafka pour déclencher un Kage Bunshin.
#[derive(Debug, Clone)]
struct KageBunshinRequest {
    pipeline_id: Uuid,
    stage_id: Uuid,
    stage_name: String,
    stage_image: String,
    stage_commands: Vec<String>,
    error_logs: String,
    repository_id: Uuid,
    commit_id: String,
}

/// Consumer Kafka dédié pour le Kage Bunshin (auto-healing).
///
/// Vegapunk Tweak #10 : file séparée des workers pipeline.
pub struct KageBunshinConsumer {
    consumer: StreamConsumer,
    cancel_token: CancellationToken,
}

impl KageBunshinConsumer {
    /// Crée un nouveau KageBunshinConsumer.
    pub fn new(
        brokers: &str,
        topic: &str,
        group: &str,
        cancel_token: CancellationToken,
    ) -> Result<Self, domain::errors::DomainError> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group)
            .set("auto.offset.reset", "latest")
            .set("enable.auto.commit", "false")
            .create()
            .map_err(|e| {
                domain::errors::DomainError::Internal(format!(
                    "KageBunshin Kafka consumer creation failed: {e}"
                ))
            })?;

        consumer
            .subscribe(&[topic])
            .map_err(|e| {
                domain::errors::DomainError::Internal(format!(
                    "KageBunshin Kafka subscribe failed: {e}"
                ))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group = %group,
            "🥷 KageBunshinConsumer initialisé (file dédiée)"
        );

        Ok(Self {
            consumer,
            cancel_token,
        })
    }

    /// Lance la boucle de consommation avec un pool de workers.
    pub async fn run(
        self,
        kage_bunshin_uc: Arc<KageBunshinUseCase>,
        pipeline_repo: Arc<dyn PipelineRepository>,
        vcs_engine: Arc<dyn VcsEngine>,
        repo_repo: Arc<dyn RepoRepository>,
        mr_repo: Arc<dyn MrRepository>,
        actor_repo: Arc<dyn ActorRepository>,
        workspace_root: PathBuf,
        worker_count: usize,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel::<KageBunshinRequest>(8);
        let rx = Arc::new(tokio::sync::Mutex::new(rx));

        // Spawner les workers
        for worker_id in 0..worker_count {
            let rx = rx.clone();
            let kage_bunshin_uc = kage_bunshin_uc.clone();
            let pipeline_repo = pipeline_repo.clone();
            let vcs_engine = vcs_engine.clone();
            let repo_repo = repo_repo.clone();
            let mr_repo = mr_repo.clone();
            let actor_repo = actor_repo.clone();
            let workspace_root = workspace_root.clone();

            tokio::spawn(async move {
                loop {
                    let request = {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    };

                    let request = match request {
                        Some(r) => r,
                        None => {
                            info!(worker_id, "🥷 KageBunshin Worker arrêté (channel fermé)");
                            break;
                        }
                    };

                    Self::process_request(
                        worker_id,
                        &request,
                        &kage_bunshin_uc,
                        &*pipeline_repo,
                        &*vcs_engine,
                        &*repo_repo,
                        &*mr_repo,
                        &*actor_repo,
                        &workspace_root,
                    )
                    .await;
                }
            });
        }

        info!(workers = worker_count, "🥷 KageBunshin Worker Pool démarré");

        // Boucle de consommation Kafka
        use futures::StreamExt;
        let mut stream = self.consumer.stream();

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("🥷 KageBunshinConsumer — shutdown gracieux");
                    break;
                }
                message = stream.next() => {
                    match message {
                        Some(Ok(msg)) => {
                            if let Some(payload) = msg.payload() {
                                match serde_json::from_slice::<serde_json::Value>(payload) {
                                    Ok(json) => {
                                        if let Some(request) = Self::parse_request(&json) {
                                            if tx.send(request).await.is_err() {
                                                warn!("⚠️ KageBunshinConsumer — worker channel fermé");
                                                break;
                                            }
                                        } else {
                                            warn!("⚠️ KageBunshinConsumer — message JSON invalide");
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "⚠️ KageBunshinConsumer — JSON invalide");
                                    }
                                }
                            }

                            // Commit offset
                            if let Err(e) = self.consumer.commit_message(&msg, CommitMode::Async) {
                                warn!(error = %e, "⚠️ KageBunshinConsumer — commit offset échoué");
                            }
                        }
                        Some(Err(e)) => {
                            warn!(error = %e, "⚠️ KageBunshinConsumer — erreur Kafka");
                        }
                        None => {
                            info!("🥷 KageBunshinConsumer — stream terminé");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Parse un message JSON en KageBunshinRequest.
    fn parse_request(json: &serde_json::Value) -> Option<KageBunshinRequest> {
        let pipeline_id = json["pipeline_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())?;
        let stage_id = json["stage_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())?;
        let stage_name = json["stage_name"].as_str()?.to_string();
        let stage_image = json["stage_image"].as_str()?.to_string();
        let stage_commands: Vec<String> = json["stage_commands"]
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let error_logs = json["error_logs"].as_str()?.to_string();
        let repository_id = json["repository_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())?;
        let commit_id = json["commit_id"].as_str()?.to_string();

        Some(KageBunshinRequest {
            pipeline_id,
            stage_id,
            stage_name,
            stage_image,
            stage_commands,
            error_logs,
            repository_id,
            commit_id,
        })
    }

    /// Traite une demande Kage Bunshin individuelle.
    ///
    /// Flow :
    /// 1. Résoudre le repository depuis la BDD
    /// 2. Checkout le code dans un workspace éphémère
    /// 3. Appeler KageBunshinUseCase::attempt_heal()
    /// 4. Nettoyer le workspace (Tweak #9)
    async fn process_request(
        worker_id: usize,
        request: &KageBunshinRequest,
        kage_bunshin_uc: &KageBunshinUseCase,
        pipeline_repo: &dyn PipelineRepository,
        vcs_engine: &dyn VcsEngine,
        repo_repo: &dyn RepoRepository,
        mr_repo: &dyn MrRepository,
        actor_repo: &dyn ActorRepository,
        workspace_root: &PathBuf,
    ) {
        info!(
            worker_id,
            pipeline_id = %request.pipeline_id,
            stage = %request.stage_name,
            "🥷 KageBunshin Worker — traitement heal"
        );

        // 0. Résoudre le repository
        let repository = match repo_repo.find_by_id(&request.repository_id).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                warn!(
                    repo_id = %request.repository_id,
                    "⚠️ Repository introuvable — heal abandonné"
                );
                Self::mark_stage_failed(pipeline_repo, &request.stage_id, "Repository not found").await;
                return;
            }
            Err(e) => {
                warn!(
                    repo_id = %request.repository_id,
                    error = %e,
                    "⚠️ Erreur BDD — heal abandonné"
                );
                Self::mark_stage_failed(pipeline_repo, &request.stage_id, &format!("DB error: {e}")).await;
                return;
            }
        };

        // 1. Init workspace VCS
        if let Err(e) = vcs_engine.init_workspace(&repository.owner_id, &request.repository_id).await {
            warn!(
                error = %e,
                "⚠️ Impossible d'initialiser le workspace VCS — heal abandonné"
            );
            Self::mark_stage_failed(pipeline_repo, &request.stage_id, &format!("VCS init failed: {e}")).await;
            return;
        }

        // 2. Créer le workspace éphémère (shadow)
        let temp_dir = match tempfile::TempDir::new() {
            Ok(d) => d,
            Err(e) => {
                error!(error = %e, "❌ Impossible de créer le répertoire temporaire shadow");
                Self::mark_stage_failed(pipeline_repo, &request.stage_id, &format!("TempDir failed: {e}")).await;
                return;
            }
        };

        let bare_git_path = find_bare_git_path(workspace_root, &request.repository_id);
        let temp_path = temp_dir.path().to_path_buf();

        // 3. Checkout code
        if let Err(e) = checkout_code_to_tempdir(&bare_git_path, &request.commit_id, &temp_path).await {
            error!(
                error = %e,
                "❌ Impossible de matérialiser le code dans le shadow workspace"
            );
            Self::mark_stage_failed(pipeline_repo, &request.stage_id, &format!("Checkout failed: {e}")).await;
            return;
        }

        // 4. Exécuter le heal
        match kage_bunshin_uc
            .attempt_heal(
                request.pipeline_id,
                request.stage_id,
                &request.stage_name,
                &request.stage_image,
                &request.stage_commands,
                &request.error_logs,
                request.repository_id,
                &request.commit_id,
                &temp_path,
            )
            .await
        {
            Ok(HealResult::Success { diagnosis, shadow_branch, .. }) => {
                info!(
                    worker_id,
                    pipeline_id = %request.pipeline_id,
                    stage = %request.stage_name,
                    shadow_branch = %shadow_branch,
                    diagnosis = %diagnosis,
                    "🥷✅ Kage Bunshin SUCCESS — heal réussi"
                );

                // Mettre à jour le stage → Healed
                if let Err(e) = pipeline_repo
                    .update_stage_status(
                        &request.stage_id,
                        domain::entities::pipeline::PipelineStageStatus::Healed,
                        None,
                        Some(chrono::Utc::now()),
                        None,
                    )
                    .await
                {
                    warn!(error = %e, "⚠️ Impossible de mettre à jour le stage status → Healed");
                }

                // Phase 41-A4 : Créer la MR auto (Vegapunk Tweak #11)
                match Self::create_heal_mr(
                    request,
                    &shadow_branch,
                    &diagnosis,
                    mr_repo,
                    actor_repo,
                    pipeline_repo,
                ).await {
                    Ok(mr) => {
                        info!(
                            mr_id = %mr.id,
                            mr_number = mr.number,
                            source = %mr.source_branch,
                            target = %mr.target_branch,
                            author = %SENSEI_ACTOR_HANDLE,
                            "🥷✨ MR auto-heal #{} créée par Sensei (Phase 41-A4)",
                            mr.number
                        );
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "⚠️ MR auto-heal non créée (le heal a quand même réussi)"
                        );
                    }
                }

                // VP-13: Finaliser le statut du pipeline après le heal
                Self::finalize_pipeline_after_heal(pipeline_repo, &request.pipeline_id).await;
            }
            Ok(HealResult::Failed { diagnosis, reason }) => {
                warn!(
                    worker_id,
                    pipeline_id = %request.pipeline_id,
                    stage = %request.stage_name,
                    diagnosis = %diagnosis,
                    reason = %reason,
                    "🥷❌ Kage Bunshin FAILED — heal échoué"
                );

                // Remettre le stage → Failure (le heal n'a pas marché)
                Self::mark_stage_failed(pipeline_repo, &request.stage_id, &reason).await;

                // VP-13: Finaliser le statut du pipeline après le heal
                Self::finalize_pipeline_after_heal(pipeline_repo, &request.pipeline_id).await;
            }
            Err(e) => {
                error!(
                    worker_id,
                    pipeline_id = %request.pipeline_id,
                    error = %e,
                    "❌ KageBunshin — erreur inattendue"
                );
                Self::mark_stage_failed(pipeline_repo, &request.stage_id, &format!("Internal error: {e}")).await;

                // VP-13: Finaliser le statut du pipeline apr\u{00e8}s le heal
                Self::finalize_pipeline_after_heal(pipeline_repo, &request.pipeline_id).await;
            }
            Ok(HealResult::Skipped) => {
                info!(
                    worker_id,
                    pipeline_id = %request.pipeline_id,
                    stage = %request.stage_name,
                    "🥷⏭️ Kage Bunshin SKIPPED — kage_bunshin non actif pour ce stage"
                );
                Self::mark_stage_failed(pipeline_repo, &request.stage_id, "Kage Bunshin skipped").await;

                // VP-13: Finaliser le statut du pipeline apr\u{00e8}s le heal
                Self::finalize_pipeline_after_heal(pipeline_repo, &request.pipeline_id).await;
            }
        }

        // 5. Nettoyage — Vegapunk Tweak #9 : Amnésie Physique
        // TempDir::drop() nettoie automatiquement, mais on le rend explicite
        // pour la lisibilité et pour s'assurer que le log est émis.
        let shadow_path = temp_dir.path().to_path_buf();
        drop(temp_dir);
        // Double vérification : si TempDir::drop() a raté, on force le nettoyage
        if shadow_path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&shadow_path) {
                warn!(
                    path = %shadow_path.display(),
                    error = %e,
                    "⚠️ Amnésie Physique : nettoyage shadow workspace échoué"
                );
            }
        }

        info!(
            worker_id,
            pipeline_id = %request.pipeline_id,
            "🧹 Shadow workspace nettoyé (Amnésie Physique)"
        );
    }

    /// Marque un stage comme Failure après l'échec du heal.
    async fn mark_stage_failed(
        pipeline_repo: &dyn PipelineRepository,
        stage_id: &Uuid,
        _reason: &str,
    ) {
        if let Err(e) = pipeline_repo
            .update_stage_status(
                stage_id,
                PipelineStageStatus::Failure,
                None,
                Some(Utc::now()),
                None,
            )
            .await
        {
            warn!(error = %e, "⚠️ Impossible de mettre à jour le stage → Failure après heal échoué");
        }
    }

    // ── VP-13 : Finalisation du pipeline après heal ──────────────────────

    /// Recalcule le statut du pipeline après la fin d'un heal.
    ///
    /// Le pipeline avait été maintenu en "running" par VP-13 tant que
    /// des stages étaient en `Healing`. Cette méthode vérifie si
    /// tous les stages ont atteint un état terminal et calcule le
    /// statut final du pipeline.
    ///
    /// États terminaux : success, failure, error, skipped, healed
    /// États non-terminaux : pending, running, healing
    async fn finalize_pipeline_after_heal(
        pipeline_repo: &dyn PipelineRepository,
        pipeline_id: &Uuid,
    ) {
        // Récupérer tous les stages du pipeline
        let stages = match pipeline_repo.list_stages(pipeline_id).await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "VP-13: Impossible de lister les stages pour finaliser le pipeline");
                return;
            }
        };

        // Vérifier s'il reste des stages non-terminaux
        let has_healing = stages.iter().any(|s| s.status == PipelineStageStatus::Healing);
        let has_running = stages.iter().any(|s| {
            s.status == PipelineStageStatus::Running || s.status == PipelineStageStatus::Pending
        });

        if has_healing || has_running {
            // Il reste des stages en cours — ne pas finaliser encore
            info!(
                pipeline_id = %pipeline_id,
                healing = has_healing,
                running = has_running,
                "🥷 Pipeline pas encore finalisé — stages en cours"
            );
            return;
        }

        // Tous les stages sont terminés — calculer le statut final
        let has_failure = stages.iter().any(|s| {
            s.status == PipelineStageStatus::Failure || s.status == PipelineStageStatus::Error
        });
        let has_error = stages.iter().any(|s| s.status == PipelineStageStatus::Error);

        let final_status = if has_error {
            PipelineStatus::Error
        } else if has_failure {
            PipelineStatus::Failure
        } else {
            // Tous les stages sont success/healed/skipped
            PipelineStatus::Success
        };

        info!(
            pipeline_id = %pipeline_id,
            final_status = ?final_status,
            "🥷 VP-13: Pipeline finalisé après heal → {:?}",
            final_status
        );

        if let Err(e) = pipeline_repo
            .update_status(
                pipeline_id,
                final_status,
                None,
                Some(Utc::now()),
                None,
            )
            .await
        {
            warn!(error = %e, "VP-13: Impossible de finaliser le statut du pipeline");
        }
    }

    // ── Phase 41-A4 : MR Auto-Healing (Vegapunk Tweak #11) ────────────

    /// Crée une MR automatique après un heal réussi.
    ///
    /// Flow :
    /// 1. Résoudre le Service Account Sensei (find or create)
    /// 2. Attribuer le numéro séquentiel de MR
    /// 3. Persister la MR via `MrRepository`
    /// 4. Émettre l'événement timeline `Opened` (auteur = Sensei)
    /// 5. Mettre à jour le `heal_attempt` avec le `mr_id`
    ///
    /// Note : Bypass RBAC — le Kage Bunshin est un processus système,
    /// pas un utilisateur soumis aux contrôles collaborateur.
    async fn create_heal_mr(
        request: &KageBunshinRequest,
        shadow_branch: &str,
        diagnosis: &str,
        mr_repo: &dyn MrRepository,
        actor_repo: &dyn ActorRepository,
        pipeline_repo: &dyn PipelineRepository,
    ) -> Result<MergeRequest, domain::errors::DomainError> {
        // 1. Résoudre le Sensei service account
        let sensei_id = Self::resolve_sensei_bot(actor_repo).await?;

        // 2. Construire la MR
        let title = format!(
            "🥷 Auto-fix: {} (pipeline {})",
            request.stage_name,
            &request.pipeline_id.to_string()[..8]
        );
        let description = Some(format!(
            "## 🥷 Kage Bunshin — Auto-Healing\n\n\
            **Stage échoué** : `{}`\n\
            **Pipeline** : `{}`\n\
            **Image** : `{}`\n\n\
            ### Diagnostic Sensei\n\
            {}\n\n\
            ---\n\
            > _Cette MR a été générée automatiquement par le Sensei Bot (Phase 41)._\n\
            > _Vérifiez les changements avant de merge._",
            request.stage_name,
            request.pipeline_id,
            request.stage_image,
            diagnosis,
        ));

        // Branche source = shadow_branch (créée par le heal)
        // Branche cible = main (ou la branche par défaut)
        let source_branch = shadow_branch.to_string();
        let target_branch = "main".to_string();

        // 3. Numéro atomique
        let number = mr_repo.next_number(&request.repository_id).await?;

        let mr = MergeRequest::new(
            request.repository_id,
            sensei_id,
            number,
            title,
            description,
            source_branch,
            target_branch,
        );

        mr_repo.save(&mr).await?;

        // 4. Événement timeline
        let event = MrEvent::new(
            mr.id,
            sensei_id,
            MrEventType::Opened,
            serde_json::json!({
                "title": &mr.title,
                "auto_heal": true,
                "pipeline_id": request.pipeline_id,
                "stage_name": &request.stage_name,
            }),
        );
        if let Err(e) = mr_repo.save_event(&event).await {
            warn!(error = %e, "⚠️ Impossible de sauver l'événement MR Opened");
        }

        // 5. Mettre à jour le heal_attempt avec le mr_id
        if let Err(e) = pipeline_repo
            .update_heal_attempt_mr(
                &request.pipeline_id,
                &request.stage_name,
                &mr.id,
            )
            .await
        {
            warn!(
                error = %e,
                mr_id = %mr.id,
                "⚠️ Impossible de lier le heal_attempt à la MR (non bloquant)"
            );
        }

        info!(
            mr_id = %mr.id,
            mr_number = mr.number,
            "🥷✨ MR #{} créée — auteur: {}",
            mr.number,
            SENSEI_ACTOR_HANDLE
        );

        Ok(mr)
    }

    /// Résout le Service Account Sensei (find or create).
    ///
    /// Si le bot `shinobi-sensei-bot` n'existe pas encore en BDD,
    /// il est créé comme un `AiAgent` rattaché au `SYSTEM_ACTOR_ID`.
    async fn resolve_sensei_bot(
        actor_repo: &dyn ActorRepository,
    ) -> Result<Uuid, domain::errors::DomainError> {
        // Chercher le bot existant
        if let Some(actor) = actor_repo.find_by_handle(SENSEI_ACTOR_HANDLE).await? {
            return Ok(actor.id);
        }

        // Première exécution : créer le Service Account
        let bot = Actor::new_service_account(
            SENSEI_ACTOR_HANDLE,
            "Sensei Auto-Healing Bot 🥷",
            SYSTEM_ACTOR_ID,
        );
        let bot_id = bot.id;
        actor_repo.save(&bot).await?;

        info!(
            bot_id = %bot_id,
            handle = %SENSEI_ACTOR_HANDLE,
            "🥷 Service Account Sensei créé (Phase 41 — première exécution)"
        );

        Ok(bot_id)
    }
}

// ── Fonctions utilitaires partagées avec JutsuConsumer ─────────────────

/// Trouve le chemin du bare Git repo pour un repo_id donné.
fn find_bare_git_path(workspace_root: &PathBuf, repo_id: &Uuid) -> PathBuf {
    let repo_id_str = repo_id.to_string();

    if let Ok(entries) = std::fs::read_dir(workspace_root) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(&repo_id_str).join(".jj/repo/store/git");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    workspace_root
        .join(&repo_id_str)
        .join(".jj/repo/store/git")
}

/// Clone le code depuis le bare Git repo vers un dossier temporaire.
async fn checkout_code_to_tempdir(
    bare_git_path: &PathBuf,
    commit_id: &str,
    temp_dir: &PathBuf,
) -> Result<(), String> {
    let bare_str = bare_git_path
        .to_str()
        .ok_or("bare_git_path contient des caractères non-UTF8")?
        .to_string();
    let temp_str = temp_dir
        .to_str()
        .ok_or("temp_dir contient des caractères non-UTF8")?
        .to_string();
    let commit = commit_id.to_string();

    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("git")
            .args(["clone", "--shared", "--no-checkout", &bare_str, &temp_str])
            .output()
            .map_err(|e| format!("git clone failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git clone failed: {stderr}"));
        }

        let output = std::process::Command::new("git")
            .args(["checkout", &commit])
            .current_dir(&temp_str)
            .output()
            .map_err(|e| format!("git checkout failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git checkout {commit} failed: {stderr}"));
        }

        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}
