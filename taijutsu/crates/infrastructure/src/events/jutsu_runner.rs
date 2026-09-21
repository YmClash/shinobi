//! Adaptateur Docker — JutsuRunner (Phase 40 — Jutsu Runner Natif) 🥷⚡
//!
//! Implémente le port `ContainerRunner` via la crate `bollard`.
//! Pilote le daemon Docker local pour exécuter des stages CI/CD
//! dans des containers OCI éphémères.
//!
//! ## Lifecycle d'un container
//! ```text
//! pull_image() → create_container() → start() → logs() → wait() → remove()
//! ```
//!
//! ## Labels Docker (Vegapunk Micro-Tweak #2)
//! Chaque container porte les labels :
//! - `shinobi.runner=true` : identifie les containers Jutsu
//! - `shinobi.pipeline.id=<uuid>` : rattache au pipeline parent
//! Cela permet un Garbage Collection des containers orphelins
//! en cas de crash de Taijutsu.
//!
//! ## Cross-platform
//! Utilise `Docker::connect_with_local_defaults()` qui gère
//! automatiquement le transport selon l'OS :
//! - Linux/macOS : Unix socket `/var/run/docker.sock`
//! - Windows : Named Pipe `//./pipe/docker_engine`

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions, RemoveContainerOptions,
    StartContainerOptions, WaitContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use futures::StreamExt;
use tokio::time::Instant;
use tracing::{debug, info, warn};

use domain::errors::DomainError;
use domain::ports::container_runner::{ContainerRunResult, ContainerRunner};

/// Runner Jutsu — exécute des stages CI/CD dans des containers Docker.
///
/// Thread-safe (`Send + Sync`) — le client `bollard::Docker` est
/// `Clone + Send + Sync` par conception.
pub struct JutsuRunner {
    docker: Docker,
}

impl JutsuRunner {
    /// Crée un nouveau JutsuRunner connecté au daemon Docker local.
    ///
    /// Utilise `connect_with_local_defaults()` pour la compatibilité
    /// cross-platform (Unix socket / Named Pipe Windows).
    ///
    /// # Errors
    /// Retourne une erreur si le daemon Docker n'est pas accessible.
    pub fn new() -> Result<Self, DomainError> {
        let docker = Docker::connect_with_local_defaults().map_err(|e| {
            DomainError::Internal(format!(
                "Docker daemon inaccessible — vérifiez que Docker Desktop est lancé: {e}"
            ))
        })?;

        info!("🥷 JutsuRunner connecté au daemon Docker");
        Ok(Self { docker })
    }

    /// Pull une image Docker si elle n'est pas déjà présente localement.
    async fn ensure_image(&self, image: &str) -> Result<(), DomainError> {
        // Vérifier si l'image existe déjà
        if self.docker.inspect_image(image).await.is_ok() {
            debug!(image = %image, "Image Docker déjà présente localement");
            return Ok(());
        }

        info!(image = %image, "⬇️ Pull de l'image Docker...");

        let options = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);
        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = &info.status {
                        debug!(status = %status, "Docker pull progress");
                    }
                }
                Err(e) => {
                    return Err(DomainError::BusinessRule(format!(
                        "Image Docker introuvable ou inaccessible: '{}' — {e}",
                        image
                    )));
                }
            }
        }

        info!(image = %image, "✅ Image Docker pulled avec succès");
        Ok(())
    }
}

#[async_trait]
impl ContainerRunner for JutsuRunner {
    async fn run_stage(
        &self,
        image: &str,
        commands: &[String],
        workspace_path: &Path,
        timeout: Duration,
        pipeline_id: &str,
    ) -> Result<ContainerRunResult, DomainError> {
        let start = Instant::now();

        // ── 1. Pull l'image si nécessaire ────────────────────────
        self.ensure_image(image).await?;

        // ── 2. Construire la commande ────────────────────────────
        // Enchaîner les jutsus avec && pour fail-fast
        let combined_cmd = commands.join(" && ");
        let cmd = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!("cd /workspace && {combined_cmd}"),
        ];

        // ── 3. Labels Docker (Vegapunk Micro-Tweak #2) ──────────
        let mut labels = HashMap::new();
        labels.insert("shinobi.runner".to_string(), "true".to_string());
        labels.insert(
            "shinobi.pipeline.id".to_string(),
            pipeline_id.to_string(),
        );

        // ── 4. Bind mount du workspace éphémère en RW ───────────
        let workspace_str = workspace_path
            .to_str()
            .ok_or_else(|| DomainError::Internal("workspace_path contient des caractères non-UTF8".to_string()))?;

        let bind_mount = format!("{workspace_str}:/workspace:rw");

        // ── 5. Créer le container ────────────────────────────────
        let container_name = format!("jutsu-{}", &pipeline_id[..8.min(pipeline_id.len())]);

        let config = Config {
            image: Some(image.to_string()),
            cmd: Some(cmd),
            labels: Some(labels),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(HostConfig {
                binds: Some(vec![bind_mount]),
                network_mode: Some("bridge".to_string()),
                memory: Some(2 * 1024 * 1024 * 1024), // 2 GB limit
                ..Default::default()
            }),
            ..Default::default()
        };

        let create_options = CreateContainerOptions {
            name: &container_name,
            platform: None,
        };

        let container = self
            .docker
            .create_container(Some(create_options), config)
            .await
            .map_err(|e| {
                DomainError::Internal(format!("Docker create_container failed: {e}"))
            })?;

        let container_id = container.id.clone();
        info!(
            container_id = %container_id,
            image = %image,
            "🐳 Container Jutsu créé"
        );

        // ── 6. Démarrer le container ─────────────────────────────
        self.docker
            .start_container(&container_id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| {
                DomainError::Internal(format!("Docker start_container failed: {e}"))
            })?;

        // ── 7. Attendre la fin avec timeout ──────────────────────
        let wait_result = tokio::time::timeout(timeout, async {
            // Collecter les logs en parallèle
            let log_opts = LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            };

            let mut log_stream = self.docker.logs(&container_id, Some(log_opts));
            let mut logs = String::new();

            while let Some(result) = log_stream.next().await {
                match result {
                    Ok(output) => {
                        let line = match &output {
                            LogOutput::StdOut { message } => {
                                String::from_utf8_lossy(message).to_string()
                            }
                            LogOutput::StdErr { message } => {
                                String::from_utf8_lossy(message).to_string()
                            }
                            _ => String::new(),
                        };
                        logs.push_str(&line);
                    }
                    Err(e) => {
                        logs.push_str(&format!("[jutsu-runner error: {e}]\n"));
                    }
                }
            }

            // Attendre le code de sortie
            let mut wait_stream = self
                .docker
                .wait_container(&container_id, None::<WaitContainerOptions<String>>);

            let exit_code = if let Some(result) = wait_stream.next().await {
                match result {
                    Ok(response) => response.status_code,
                    Err(e) => {
                        warn!(error = %e, "Docker wait_container error");
                        -1
                    }
                }
            } else {
                -1
            };

            (exit_code, logs)
        })
        .await;

        let (exit_code, logs) = match wait_result {
            Ok((code, logs)) => (code, logs),
            Err(_) => {
                // Timeout — kill le container
                warn!(
                    container_id = %container_id,
                    timeout_secs = timeout.as_secs(),
                    "⏰ Stage timeout — kill du container"
                );
                let _ = self.docker.kill_container::<String>(&container_id, None).await;
                (-1, format!("[jutsu-runner] Stage timeout après {}s\n", timeout.as_secs()))
            }
        };

        // ── 8. Supprimer le container (nettoyage systématique) ───
        let remove_opts = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        if let Err(e) = self.docker.remove_container(&container_id, Some(remove_opts)).await {
            warn!(
                container_id = %container_id,
                error = %e,
                "⚠️ Échec suppression container (non-fatal)"
            );
        }

        let duration_ms = start.elapsed().as_millis() as i64;

        info!(
            container_id = %container_id,
            exit_code = exit_code,
            duration_ms = duration_ms,
            "🏁 Stage terminé (exit_code={exit_code})"
        );

        Ok(ContainerRunResult {
            exit_code,
            logs,
            duration_ms,
        })
    }
}
