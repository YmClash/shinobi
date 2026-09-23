//! Pilier 3 — `anbu jutsu run --local` : Émulateur Docker local 🥷⚡
//!
//! Exécute le pipeline `jutsu.yml` directement sur la machine du développeur
//! via Docker local (crate `bollard`).
//!
//! ## Différences avec Taijutsu (serveur)
//!
//! | Aspect          | Serveur (Taijutsu)            | Local (ANBU)              |
//! |-----------------|-------------------------------|---------------------------|
//! | Workspace       | `git clone --shared` → TmpDir | Répertoire courant (cwd)  |
//! | BDD             | PostgreSQL                    | Aucune (stdout)           |
//! | Commit Status   | API REST → badge Makimono     | Exit code (0/1)           |
//! | Logs            | Tronqués HEAD/TAIL 64KB       | Streaming brut complet    |
//! | Trigger         | Kafka event                   | Appel direct bollard      |
//! | Auth            | JWT/PAT                       | Docker socket local       |
//!
//! ## Vegapunk Tweak #3 — Permissions Docker
//! Sur Linux/macOS, le container est exécuté avec l'UID:GID de l'utilisateur
//! courant pour éviter que les fichiers générés (target/, node_modules/)
//! appartiennent à root.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use bollard::container::{
    Config, CreateContainerOptions, LogOutput, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::HostConfig;
use bollard::Docker;
use colored::Colorize;
use futures::StreamExt;

use crate::config::AnbuConfig;
use super::parser;

/// Timeout par défaut pour un stage local (30 minutes).
const LOCAL_STAGE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Handler pour `anbu jutsu run`.
pub async fn cmd_run(
    _config: AnbuConfig,
    local: bool,
    stage_filter: Option<String>,
    check: bool,
    file: Option<String>,
) -> Result<()> {
    if !local && !check {
        bail!(
            "Use --local to run the pipeline locally.\n\
             Use --check to validate the YAML without executing.\n\n\
             Example:\n  \
             anbu jutsu run --local\n  \
             anbu jutsu run --local --check"
        );
    }

    // 1. Lire jutsu.yml
    let yaml_path = file.unwrap_or_else(|| "jutsu.yml".to_string());
    let yaml_content = std::fs::read_to_string(&yaml_path)
        .map_err(|e| anyhow::anyhow!(
            "Cannot read '{}': {e}\n\n\
             Create a jutsu.yml at the root of your project.\n\
             See: Docs/examples/jutsu.yml",
            yaml_path
        ))?;

    // 2. Parser + valider
    let config = parser::parse_and_validate(&yaml_content)?;

    println!();
    println!(
        "  {} {}",
        "📜 jutsu.yml".bold(),
        "validated successfully".green()
    );
    println!(
        "  Pipeline: {}",
        config.name.white().bold()
    );
    println!(
        "  Stages:   {}",
        config.stages.len().to_string().cyan()
    );
    println!(
        "  Triggers: {}",
        config.on.join(", ").dimmed()
    );

    // 3. Mode --check : afficher la config et sortir
    if check {
        println!();
        println!("  {}", "Stages:".bold());
        for (name, stage) in &config.stages {
            let deps = if stage.requires.is_empty() {
                String::new()
            } else {
                format!(" (requires: {})", stage.requires.join(", "))
            };
            println!(
                "    {} {} ({}){}",
                "▸".cyan(),
                name.white().bold(),
                stage.image.dimmed(),
                deps.dimmed()
            );
            for jutsu in &stage.jutsus {
                println!("      $ {}", jutsu.dimmed());
            }
        }
        println!();
        println!("  {} YAML is valid. No execution performed.", "✅".green());
        println!();
        return Ok(());
    }

    // 4. Connexion Docker
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow::anyhow!(
            "Docker daemon inaccessible: {e}\n\n\
             Ensure Docker Desktop is running.\n\
             Linux: sudo systemctl start docker"
        ))?;

    // Vérifier la connectivité Docker
    docker.ping().await.map_err(|e| {
        anyhow::anyhow!(
            "Cannot ping Docker daemon: {e}\n\n\
             Ensure Docker Desktop is running."
        )
    })?;

    println!(
        "  {} Docker connected",
        "🐳".bold()
    );

    // 5. Résoudre le workspace (répertoire courant)
    let workspace = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("Cannot resolve current directory: {e}"))?;

    println!(
        "  Workspace: {}",
        workspace.display().to_string().dimmed()
    );

    // 6. Résoudre UID:GID pour le Vegapunk Tweak #3
    let user_spec = resolve_user_spec();

    println!();
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    // 7. Exécution séquentielle avec Dynamic Scheduling
    let mut completed: HashSet<String> = HashSet::new();
    let mut failed: HashSet<String> = HashSet::new();
    let mut pipeline_success = true;

    let pipeline_name = config.name.clone();
    let stages: Vec<(String, super::config::JutsuStage)> = config.stages.into_iter().collect();
    let total_stages = stages.len();

    for (name, stage_def) in &stages {
        // Filtre sur --stage si spécifié
        if let Some(ref filter) = stage_filter {
            if name != filter {
                continue;
            }
        }

        // Vérifier les dépendances
        let deps_failed = stage_def.requires.iter().any(|dep| failed.contains(dep));
        let deps_satisfied = stage_def.requires.iter().all(|dep| completed.contains(dep));

        if deps_failed || (!deps_satisfied && !stage_def.requires.is_empty()) {
            println!(
                "  {} {} — {} (dependency failed)",
                "⏭️",
                name.white().bold(),
                "skipped".dimmed()
            );
            failed.insert(name.clone());
            continue;
        }

        println!(
            "  {} {} ({}) — {}",
            "▶".yellow().bold(),
            name.white().bold(),
            stage_def.image.dimmed(),
            "running".yellow()
        );

        // Exécuter le stage
        let result = run_stage(
            &docker,
            &stage_def.image,
            &stage_def.jutsus,
            &workspace,
            &name,
            user_spec.as_deref(),
        ).await;

        match result {
            Ok(exit_code) => {
                if exit_code == 0 {
                    completed.insert(name.clone());
                    println!(
                        "  {} {} — {} ✅",
                        "✓".green().bold(),
                        name.white(),
                        "success".green().bold()
                    );
                } else {
                    failed.insert(name.clone());
                    pipeline_success = false;
                    println!(
                        "  {} {} — {} (exit code: {})",
                        "✗".red().bold(),
                        name.white(),
                        "failure".red().bold(),
                        exit_code
                    );
                }
            }
            Err(e) => {
                failed.insert(name.clone());
                pipeline_success = false;
                eprintln!(
                    "  {} {} — {} {}",
                    "⚠️",
                    name.white(),
                    "error".red(),
                    e.to_string().red()
                );
            }
        }
    }

    // 8. Résumé final
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    if pipeline_success {
        println!(
            "  {} Pipeline '{}' — {} ({}/{} stages)",
            "🥷",
            pipeline_name.white().bold(),
            "ALL GREEN".green().bold(),
            completed.len(),
            total_stages
        );
    } else {
        println!(
            "  {} Pipeline '{}' — {} ({}/{} succeeded, {} failed)",
            "🥷",
            pipeline_name.white().bold(),
            "FAILED".red().bold(),
            completed.len(),
            total_stages,
            failed.len()
        );
    }
    println!();

    // Exit code process = 0 si tout vert, 1 sinon
    if !pipeline_success {
        std::process::exit(1);
    }

    Ok(())
}

// ── Docker Stage Execution ──────────────────────────────────────────────

/// Exécute un stage dans un container Docker local.
///
/// Adapté de `taijutsu/crates/infrastructure/src/events/jutsu_runner.rs`.
/// Différences clés :
/// - Logs streamés directement vers stdout (pas d'accumulation mémoire)
/// - Workspace = cwd (pas de git clone)
/// - Pas de labels `shinobi.pipeline.id` (labels `shinobi.local=true`)
async fn run_stage(
    docker: &Docker,
    image: &str,
    commands: &[String],
    workspace: &PathBuf,
    stage_name: &str,
    user_spec: Option<&str>,
) -> Result<i64> {
    // 1. Pull l'image si nécessaire
    ensure_image(docker, image).await?;

    // 2. Construire la commande (enchaîner avec && pour fail-fast)
    let combined_cmd = commands.join(" && ");
    let cmd = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!("cd /workspace && {combined_cmd}"),
    ];

    // 3. Labels Docker
    let mut labels = std::collections::HashMap::new();
    labels.insert("shinobi.local".to_string(), "true".to_string());
    labels.insert("shinobi.stage".to_string(), stage_name.to_string());

    // 4. Bind mount du workspace
    let workspace_str = workspace.to_str()
        .ok_or_else(|| anyhow::anyhow!("Workspace path contains non-UTF8 characters"))?;
    let bind_mount = format!("{workspace_str}:/workspace:rw");

    // 5. Container config
    let container_name = format!("jutsu-local-{}", &stage_name.to_lowercase());

    let container_config = Config {
        image: Some(image.to_string()),
        cmd: Some(cmd),
        labels: Some(labels),
        working_dir: Some("/workspace".to_string()),
        // Vegapunk Tweak #3 : UID:GID pour éviter les fichiers root
        user: user_spec.map(|s| s.to_string()),
        host_config: Some(HostConfig {
            binds: Some(vec![bind_mount]),
            network_mode: Some("bridge".to_string()),
            memory: Some(4 * 1024 * 1024 * 1024), // 4 GB limit (local = plus généreux)
            ..Default::default()
        }),
        ..Default::default()
    };

    let create_options = CreateContainerOptions {
        name: &container_name,
        platform: None,
    };

    // Supprimer le container s'il existe déjà (re-run)
    let _ = docker.remove_container(
        &container_name,
        Some(RemoveContainerOptions { force: true, ..Default::default() }),
    ).await;

    // 6. Créer le container
    let container = docker
        .create_container(Some(create_options), container_config)
        .await
        .map_err(|e| anyhow::anyhow!("Docker create_container failed: {e}"))?;

    let container_id = container.id.clone();

    // 7. Démarrer le container
    docker
        .start_container(&container_id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| anyhow::anyhow!("Docker start_container failed: {e}"))?;

    // 8. Streamer les logs en temps réel (pas d'accumulation mémoire)
    let exit_code = tokio::time::timeout(LOCAL_STAGE_TIMEOUT, async {
        // Logs streaming brut
        let log_opts = LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            ..Default::default()
        };

        let mut log_stream = docker.logs(&container_id, Some(log_opts));

        while let Some(result) = log_stream.next().await {
            match result {
                Ok(output) => {
                    let text = match &output {
                        LogOutput::StdOut { message } => {
                            String::from_utf8_lossy(message).to_string()
                        }
                        LogOutput::StdErr { message } => {
                            String::from_utf8_lossy(message).to_string()
                        }
                        _ => String::new(),
                    };
                    // Streaming brut : pas de troncature, préservation ANSI
                    if !text.is_empty() {
                        print!("    {text}");
                    }
                }
                Err(e) => {
                    eprintln!("    [jutsu-runner error: {e}]");
                }
            }
        }

        // Attendre le code de sortie
        let mut wait_stream = docker
            .wait_container(&container_id, None::<WaitContainerOptions<String>>);

        if let Some(result) = wait_stream.next().await {
            match result {
                Ok(response) => response.status_code,
                Err(e) => {
                    eprintln!("    [docker wait error: {e}]");
                    -1
                }
            }
        } else {
            -1
        }
    }).await;

    let exit_code = match exit_code {
        Ok(code) => code,
        Err(_) => {
            eprintln!(
                "    {} Stage timeout ({}s) — killing container",
                "⏰".yellow(),
                LOCAL_STAGE_TIMEOUT.as_secs()
            );
            let _ = docker.kill_container::<String>(&container_id, None).await;
            -1
        }
    };

    // 9. Supprimer le container (nettoyage)
    let _ = docker.remove_container(
        &container_id,
        Some(RemoveContainerOptions { force: true, ..Default::default() }),
    ).await;

    Ok(exit_code)
}

/// Pull une image Docker si elle n'est pas déjà présente localement.
async fn ensure_image(docker: &Docker, image: &str) -> Result<()> {
    // Vérifier si l'image existe déjà
    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }

    println!(
        "    {} Pulling {}...",
        "⬇️".dimmed(),
        image.cyan()
    );

    let options = CreateImageOptions {
        from_image: image,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(result) = stream.next().await {
        match result {
            Ok(_info) => {
                // Progress silencieux — on pourrait ajouter une barre ici
            }
            Err(e) => {
                bail!(
                    "Docker image '{}' introuvable ou inaccessible: {e}",
                    image
                );
            }
        }
    }

    println!(
        "    {} Image pulled successfully",
        "✓".green()
    );

    Ok(())
}

/// Résout le UID:GID de l'utilisateur courant (Linux/macOS).
///
/// ## Vegapunk Tweak #3 — Permissions Docker
/// Sur Linux/macOS, passe `UID:GID` au container pour que les fichiers
/// générés par le build (target/, node_modules/) appartiennent au
/// développeur et non à root.
///
/// Sur Windows, retourne `None` (Docker Desktop gère les permissions).
fn resolve_user_spec() -> Option<String> {
    #[cfg(unix)]
    {
        use std::process::Command;

        let uid = Command::new("id").arg("-u").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        let gid = Command::new("id").arg("-g").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        match (uid, gid) {
            (Some(u), Some(g)) if !u.is_empty() && !g.is_empty() => {
                Some(format!("{u}:{g}"))
            }
            _ => None,
        }
    }

    #[cfg(not(unix))]
    {
        None
    }
}
