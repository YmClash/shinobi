//! Consumer Kafka — JutsuConsumer (Phase 40 — Jutsu Runner Natif) 🥷⚡
//!
//! Écoute le topic `shinobi.jutsu.pipeline` et dispatch les événements
//! vers un pool de workers qui exécutent les pipelines CI/CD natifs.
//!
//! ## Architecture
//! ```text
//! Kafka Topic → JutsuConsumer → mpsc channel → Worker Pool
//!   ↓                                              ↓
//! Messages JSON                              RunPipelineUseCase
//! ```
//!
//! ## Pattern
//! - Consumer Kafka rdkafka (`StreamConsumer`)
//! - mpsc bounded channel pour découpler la consommation des workers
//! - Chaque worker : parse jutsu.yml → valide → exécute le pipeline
//! - Workspace éphémère via `tempfile::TempDir` (Vegapunk Tweak #1)
//! - Graceful shutdown via `CancellationToken`

use std::path::PathBuf;
use std::sync::Arc;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Message;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use application::use_cases::parse_jutsu_config::ParseJutsuConfigUseCase;
use application::use_cases::run_pipeline::RunPipelineUseCase;
use domain::ports::vcs_engine::VcsEngine;

/// Message reçu depuis Kafka pour déclencher un pipeline.
#[derive(Debug, Clone)]
struct PipelineRequest {
    repository_id: Uuid,
    commit_id: String,
    trigger_event: String,
}

/// Consumer Kafka pour le Jutsu Runner.
///
/// Écoute le topic `shinobi.jutsu.pipeline` et dispatch les pipelines
/// vers un pool de workers.
pub struct JutsuConsumer {
    consumer: StreamConsumer,
    cancel_token: CancellationToken,
}

impl JutsuConsumer {
    /// Crée un nouveau JutsuConsumer.
    ///
    /// # Arguments
    /// - `brokers` : liste de brokers Kafka
    /// - `topic` : topic à écouter (ex: "shinobi.jutsu.pipeline")
    /// - `group` : consumer group (ex: "shinobi-jutsu-runner")
    /// - `cancel_token` : token de shutdown gracieux
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
                    "Jutsu Kafka consumer creation failed: {e}"
                ))
            })?;

        consumer
            .subscribe(&[topic])
            .map_err(|e| {
                domain::errors::DomainError::Internal(format!(
                    "Jutsu Kafka subscribe failed: {e}"
                ))
            })?;

        info!(
            brokers = %brokers,
            topic = %topic,
            group = %group,
            "🥷 JutsuConsumer initialisé"
        );

        Ok(Self {
            consumer,
            cancel_token,
        })
    }

    /// Lance la boucle de consommation avec un pool de workers.
    ///
    /// # Vegapunk Tweaks intégrés
    /// - **#1** : Workspace éphémère via `tempfile::TempDir` (cross-platform)
    /// - **#3** : `jj workspace forget` avant nettoyage (TODO: à implémenter quand VcsEngine supportera)
    pub async fn run(
        self,
        run_pipeline: Arc<RunPipelineUseCase>,
        parse_config: Arc<ParseJutsuConfigUseCase>,
        vcs_engine: Arc<dyn VcsEngine>,
        workspace_root: PathBuf,
        worker_count: usize,
    ) {
        let (tx, rx) = mpsc::channel::<PipelineRequest>(16);
        let rx = Arc::new(tokio::sync::Mutex::new(rx));

        // Spawner les workers
        for worker_id in 0..worker_count {
            let rx = rx.clone();
            let run_pipeline = run_pipeline.clone();
            let parse_config = parse_config.clone();
            let vcs_engine = vcs_engine.clone();
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
                            info!(worker_id, "🥷 Jutsu Worker arrêté (channel fermé)");
                            break;
                        }
                    };

                    Self::process_request(
                        worker_id,
                        &request,
                        &run_pipeline,
                        &parse_config,
                        &*vcs_engine,
                        &workspace_root,
                    )
                    .await;
                }
            });
        }

        info!(workers = worker_count, "🥷 Jutsu Worker Pool démarré");

        // Boucle de consommation Kafka
        use futures::StreamExt;
        let mut stream = self.consumer.stream();

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!("🥷 JutsuConsumer — shutdown gracieux");
                    break;
                }
                message = stream.next() => {
                    match message {
                        Some(Ok(msg)) => {
                            if let Some(payload) = msg.payload() {
                                match serde_json::from_slice::<serde_json::Value>(payload) {
                                    Ok(json) => {
                                        let repo_id_str = json["repository_id"].as_str().unwrap_or("");
                                        let commit_id = json["commit_id"].as_str().unwrap_or("").to_string();
                                        let trigger = json["trigger_event"].as_str().unwrap_or("push").to_string();

                                        if let Ok(repo_id) = Uuid::parse_str(repo_id_str) {
                                            let request = PipelineRequest {
                                                repository_id: repo_id,
                                                commit_id,
                                                trigger_event: trigger,
                                            };

                                            if tx.send(request).await.is_err() {
                                                warn!("⚠️ JutsuConsumer — worker channel fermé");
                                                break;
                                            }
                                        } else {
                                            warn!(
                                                raw_id = %repo_id_str,
                                                "⚠️ JutsuConsumer — repository_id invalide"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "⚠️ JutsuConsumer — JSON invalide");
                                    }
                                }
                            }

                            // Commit offset
                            if let Err(e) = self.consumer.commit_message(&msg, CommitMode::Async) {
                                warn!(error = %e, "⚠️ JutsuConsumer — commit offset échoué");
                            }
                        }
                        Some(Err(e)) => {
                            warn!(error = %e, "⚠️ JutsuConsumer — erreur Kafka");
                        }
                        None => {
                            info!("🥷 JutsuConsumer — stream terminé");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Traite une demande de pipeline individuelle.
    async fn process_request(
        worker_id: usize,
        request: &PipelineRequest,
        run_pipeline: &RunPipelineUseCase,
        parse_config: &ParseJutsuConfigUseCase,
        vcs_engine: &dyn VcsEngine,
        workspace_root: &PathBuf,
    ) {
        info!(
            worker_id,
            repo_id = %request.repository_id,
            commit_id = %request.commit_id,
            trigger = %request.trigger_event,
            "🥷 Jutsu Worker — traitement pipeline"
        );

        // 1. Lire le jutsu.yml depuis le dépôt
        let yaml_content = match vcs_engine
            .read_blob(&request.repository_id, &request.commit_id, "jutsu.yml")
            .await
        {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        repo_id = %request.repository_id,
                        error = %e,
                        "⚠️ jutsu.yml contient des octets non-UTF8 — ignoré"
                    );
                    return;
                }
            },
            Err(e) => {
                warn!(
                    repo_id = %request.repository_id,
                    error = %e,
                    "⚠️ jutsu.yml introuvable ou illisible — pipeline ignoré"
                );
                return;
            }
        };

        // 2. Parser et valider le jutsu.yml
        let config = match parse_config.parse_and_validate(&yaml_content) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    repo_id = %request.repository_id,
                    error = %e,
                    "⚠️ jutsu.yml invalide — pipeline ignoré"
                );
                return;
            }
        };

        // 3. Vérifier que le trigger correspond
        if !config.on.contains(&request.trigger_event) {
            info!(
                repo_id = %request.repository_id,
                trigger = %request.trigger_event,
                allowed = ?config.on,
                "ℹ️ Trigger non listé dans 'on:' — pipeline ignoré"
            );
            return;
        }

        // 4. Créer le workspace éphémère (Vegapunk Tweak #1)
        // Utilise tempfile::TempDir pour la gestion automatique cross-platform
        let temp_dir = match tempfile::TempDir::new() {
            Ok(d) => d,
            Err(e) => {
                error!(
                    error = %e,
                    "❌ Impossible de créer le répertoire temporaire pour le workspace CI"
                );
                return;
            }
        };

        // Matérialiser le code source via git clone --shared depuis le bare repo
        // Le bare repo est à : {workspace_root}/{owner_id}/{repo_id}/.jj/repo/store/git
        // Pour V1, on utilise une approche simple : copier les fichiers via read_blob
        // ou git archive. Ici on utilise un git clone local depuis le bare repo.
        let bare_git_path = find_bare_git_path(workspace_root, &request.repository_id);
        let temp_path = temp_dir.path().to_path_buf();

        if let Err(e) = checkout_code_to_tempdir(&bare_git_path, &request.commit_id, &temp_path).await {
            error!(
                repo_id = %request.repository_id,
                commit_id = %request.commit_id,
                error = %e,
                "❌ Impossible de matérialiser le code dans le workspace éphémère"
            );
            return;
        }

        // 5. Exécuter le pipeline
        // TODO: résoudre owner/repo_name depuis le repo_id (pour le commit status context)
        let owner = "system";
        let repo_name = &request.repository_id.to_string()[..8];

        match run_pipeline
            .execute(
                owner,
                repo_name,
                request.repository_id,
                &request.commit_id,
                &request.trigger_event,
                config.clone(),
                None,
                &temp_path,
            )
            .await
        {
            Ok(pipeline) => {
                info!(
                    worker_id,
                    pipeline_id = %pipeline.id,
                    status = %pipeline.status,
                    "🏁 Pipeline terminé avec succès"
                );
            }
            Err(e) => {
                error!(
                    worker_id,
                    repo_id = %request.repository_id,
                    error = %e,
                    "❌ Pipeline execution failed"
                );
            }
        }

        // 6. Nettoyage automatique — TempDir::drop() supprime le dossier
        //    Vegapunk Tweak #3 : `jj workspace forget` n'est pas nécessaire ici
        //    car on utilise un git clone local, pas un jj workspace add.
        drop(temp_dir);
    }
}

/// Trouve le chemin du bare Git repo pour un repo_id donné.
///
/// Cherche le pattern `{workspace_root}/*/{repo_id}/.jj/repo/store/git`
/// dans les sous-dossiers owner_id.
fn find_bare_git_path(workspace_root: &PathBuf, repo_id: &Uuid) -> PathBuf {
    let repo_id_str = repo_id.to_string();

    // Scanner les owner directories
    if let Ok(entries) = std::fs::read_dir(workspace_root) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(&repo_id_str).join(".jj/repo/store/git");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // Fallback : chemin plat (legacy)
    workspace_root
        .join(&repo_id_str)
        .join(".jj/repo/store/git")
}

/// Clone le code depuis le bare Git repo vers un dossier temporaire.
///
/// Utilise `git clone --shared --no-checkout` + `git checkout <commit>`.
/// Le `--shared` évite de copier les objets Git (utilise des alternates).
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
        // git clone --shared --no-checkout <bare> <tempdir>
        let output = std::process::Command::new("git")
            .args(["clone", "--shared", "--no-checkout", &bare_str, &temp_str])
            .output()
            .map_err(|e| format!("git clone failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git clone failed: {stderr}"));
        }

        // git checkout <commit_id>
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
