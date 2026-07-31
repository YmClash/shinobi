//! # ANBU 暗部 — AI Context Capture CLI for SHINOBI
//!
//! Point d'entrée du CLI. Route les commandes vers les handlers appropriés.
//!
//! ## Architecture
//! ```text
//! main.rs → cli.rs (clap parsing)
//!         → collectors/ (scan brain/)
//!         → storage/ (local staging + SQLite index)
//!         → vcs/ (jj describe trailers)
//! ```

mod cli;
mod collectors;
mod config;
mod models;
mod storage;
mod sync;
mod vcs;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::cli::{Cli, Commands};
use crate::collectors::antigravity::AntigravityCollector;
use crate::collectors::Collector;
use crate::config::AnbuConfig;
use crate::storage::artifact_store::ArtifactStore;
use crate::storage::index::AnbuIndex;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = AnbuConfig::load()?;

    match cli.command {
        Some(Commands::Checkpoint {
            session,
            latest,
            message,
            attach,
            revision,
            no_tag,
        }) => cmd_checkpoint(config, session, latest, message, attach, revision, no_tag),
        Some(Commands::Log { limit }) => cmd_log(config, limit),
        Some(Commands::Show { id, artifact }) => cmd_show(config, id, artifact),
        Some(Commands::Sessions { limit, agent: _ }) => cmd_sessions(config, limit),
        Some(Commands::Sync { owner, repo, id }) => cmd_sync(config, owner, repo, id),
        None => {
            // Friendly welcome banner when no subcommand is given
            println!();
            println!(
                "  {} v{}",
                "🥷 ANBU 暗部".bold(),
                env!("CARGO_PKG_VERSION")
            );
            println!(
                "  {}",
                "AI Context Capture for SHINOBI".dimmed()
            );
            println!();
            println!("  {}", "Commands:".bold());
            println!("    {}   Capture AI artifacts & tag commit", "checkpoint".cyan());
            println!("    {}         List saved checkpoints", "log".cyan());
            println!("    {}        Show checkpoint details", "show".cyan());
            println!("    {}    List detected AI sessions", "sessions".cyan());
            println!();
            println!(
                "  Quick start: {}",
                "anbu sessions".cyan().bold()
            );
            println!(
                "  Full help:   {}",
                "anbu --help".dimmed()
            );
            println!();
            Ok(())
        }
    }
}

// ── Command Handlers ─────────────────────────────────────────────────────

fn cmd_checkpoint(
    config: AnbuConfig,
    session: Option<String>,
    latest: bool,
    message: Option<String>,
    attach: Vec<std::path::PathBuf>,
    revision: String,
    no_tag: bool,
) -> Result<()> {
    let collector = AntigravityCollector::new(config.brain_path());

    // Résoudre la session
    let session_id = if let Some(sid) = session {
        sid
    } else if latest {
        let sessions = collector.detect_sessions()?;
        if sessions.is_empty() {
            anyhow::bail!("No AI sessions detected. Check your Antigravity brain path.");
        }
        sessions[0].id.clone()
    } else if !attach.is_empty() {
        // Mode --attach sans session : crée un checkpoint manuel
        "manual".to_string()
    } else {
        anyhow::bail!(
            "Specify --session <id> or --latest to select a session.\n\
             Use `anbu sessions` to list available sessions."
        );
    };

    println!(
        "{} Capturing session {}...",
        "🥷".bold(),
        session_id[..8.min(session_id.len())].cyan()
    );

    // Collecter les artifacts
    let mut collected = if session_id != "manual" {
        collector.collect_session(&session_id)?
    } else {
        Vec::new()
    };

    // Ajouter les fichiers manuels --attach
    for path in &attach {
        if path.exists() {
            let artifact = collectors::create_manual_artifact(path)?;
            collected.push(artifact);
        } else {
            eprintln!(
                "  {} File not found: {}",
                "⚠".yellow(),
                path.display()
            );
        }
    }

    if collected.is_empty() {
        println!("  {} No artifacts found.", "ℹ".blue());
        return Ok(());
    }

    println!(
        "  {} {} artifact(s) collected",
        "✓".green(),
        collected.len()
    );

    // Stocker localement
    let store = ArtifactStore::new()?;

    // Auto-inject .gitignore (safeguard Vegapunk A)
    store.ensure_gitignore()?;

    let checkpoint = store.create_checkpoint(
        models::AgentKind::Antigravity,
        &session_id,
        message.as_deref(),
        collected,
    )?;

    println!(
        "  {} Checkpoint {} created",
        "✓".green(),
        checkpoint.id.to_string()[..8].cyan()
    );

    // Indexer dans SQLite
    let index = AnbuIndex::open(&config.database_path())?;
    index.insert_checkpoint(&checkpoint)?;
    println!("  {} Indexed in local database", "✓".green());

    // Attacher les trailers jj (sauf --no-tag)
    if !no_tag {
        match vcs::jj_integration::attach_trailers(&revision, &checkpoint) {
            Ok(()) => {
                println!(
                    "  {} Trailers attached to revision {}",
                    "✓".green(),
                    revision.cyan()
                );
            }
            Err(e) => {
                eprintln!(
                    "  {} Failed to attach jj trailers: {e}\n    \
                     (Not in a jj repo? Use --no-tag to skip)",
                    "⚠".yellow()
                );
            }
        }
    }

    // Summary
    println!();
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!(
        "  🥷 {} {}",
        "Checkpoint".bold(),
        checkpoint.id.to_string()[..8].cyan().bold()
    );
    println!(
        "  Agent: {} │ Session: {}",
        checkpoint.agent.to_string().green(),
        checkpoint.session_id[..8.min(checkpoint.session_id.len())].dimmed()
    );
    if let Some(ref msg) = checkpoint.message {
        println!("  Message: {}", msg.white().bold());
    }
    println!("  Artifacts:");
    for a in &checkpoint.artifacts {
        let size = format_size(a.size_bytes);
        println!("    {} {} ({})", a.kind, a.filename.white(), size.dimmed());
    }
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    Ok(())
}

fn cmd_log(config: AnbuConfig, limit: usize) -> Result<()> {
    let index = AnbuIndex::open(&config.database_path())?;
    let checkpoints = index.list_checkpoints(limit)?;

    if checkpoints.is_empty() {
        println!("  {} No checkpoints found.", "ℹ".blue());
        println!(
            "  Use {} to create one.",
            "anbu checkpoint --latest".cyan()
        );
        return Ok(());
    }

    println!();
    println!(
        " {} {} checkpoint(s)",
        "🥷 ANBU Log".bold(),
        checkpoints.len()
    );
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    for cp in &checkpoints {
        let short_id = &cp.id.to_string()[..8];
        let date = cp.created_at.format("%Y-%m-%d %H:%M");
        let msg = cp
            .message
            .as_deref()
            .unwrap_or("(no message)")
            ;

        println!(
            "  {}  {}  {}  {}",
            short_id.cyan().bold(),
            date.to_string().dimmed(),
            cp.agent.to_string().green(),
            msg.white()
        );

        let artifact_summary: Vec<String> = cp
            .artifacts
            .iter()
            .map(|a| format!("{}", a.kind))
            .collect();
        let commit_info = cp
            .commit_id
            .as_deref()
            .map(|c| format!("→ commit {}", &c[..7.min(c.len())]))
            .unwrap_or_default();

        println!(
            "           {} │ {}",
            commit_info.dimmed(),
            artifact_summary.join(" + ").dimmed()
        );
    }

    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    Ok(())
}

fn cmd_show(config: AnbuConfig, id: String, artifact_name: Option<String>) -> Result<()> {
    let index = AnbuIndex::open(&config.database_path())?;
    let checkpoint = index.find_checkpoint(&id)?;

    if let Some(ref artifact_file) = artifact_name {
        // Afficher le contenu d'un artifact spécifique
        let artifact = checkpoint
            .artifacts
            .iter()
            .find(|a| a.filename == *artifact_file)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Artifact '{}' not found in checkpoint {}",
                    artifact_file,
                    &checkpoint.id.to_string()[..8]
                )
            })?;

        if artifact.stored_path.exists() {
            let content = std::fs::read_to_string(&artifact.stored_path)?;
            println!("{content}");
        } else {
            anyhow::bail!(
                "Artifact file not found at: {}",
                artifact.stored_path.display()
            );
        }
    } else {
        // Afficher le résumé du checkpoint
        println!();
        println!(
            "  🥷 {} {}",
            "Checkpoint".bold(),
            checkpoint.id.to_string().cyan()
        );
        println!(
            "{}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
        );
        println!(
            "  Agent:    {}",
            checkpoint.agent.to_string().green()
        );
        println!(
            "  Session:  {}",
            checkpoint.session_id.dimmed()
        );
        if let Some(ref commit) = checkpoint.commit_id {
            println!("  Commit:   {}", commit.cyan());
        }
        if let Some(ref msg) = checkpoint.message {
            println!("  Message:  {}", msg.white().bold());
        }
        println!(
            "  Created:  {}",
            checkpoint.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!();
        println!("  {}:", "Artifacts".bold());
        for a in &checkpoint.artifacts {
            let size = format_size(a.size_bytes);
            let exists = if a.stored_path.exists() { "✓" } else { "✗" };
            println!(
                "    {} {} {} ({})",
                exists.green(),
                a.kind,
                a.filename.white(),
                size.dimmed()
            );
        }
        println!();
        println!(
            "  Use {} to view an artifact.",
            format!("anbu show {} --artifact <name>", &checkpoint.id.to_string()[..8])
                .cyan()
        );
    }

    Ok(())
}

fn cmd_sessions(config: AnbuConfig, limit: usize) -> Result<()> {
    let collector = AntigravityCollector::new(config.brain_path());
    let sessions = collector.detect_sessions()?;

    if sessions.is_empty() {
        println!(
            "  {} No AI sessions detected at {}",
            "ℹ".blue(),
            config.brain_path().display()
        );
        return Ok(());
    }

    let display_count = limit.min(sessions.len());

    println!();
    println!(
        " {} {} session(s) detected (showing {})",
        "🧠 Antigravity Sessions".bold(),
        sessions.len(),
        display_count
    );
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    for session in sessions.iter().take(display_count) {
        let short_id = &session.id[..8.min(session.id.len())];
        let date = session.last_modified.format("%Y-%m-%d %H:%M");
        let summary = session
            .summary
            .as_deref()
            .unwrap_or("(no summary)")
            ;
        let truncated = if summary.len() > 60 {
            format!("{}...", &summary[..57])
        } else {
            summary.to_string()
        };

        println!(
            "  {}  {}  {} artifact(s)  {}",
            short_id.cyan().bold(),
            date.to_string().dimmed(),
            session.artifact_count.to_string().green(),
            truncated.white()
        );
    }

    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!();
    println!(
        "  Capture with: {}",
        "anbu checkpoint --session <id>".cyan()
    );

    Ok(())
}

// ── Sync Command ─────────────────────────────────────────────────────────

/// Handler: `anbu sync --owner <owner> --repo <repo>`
///
/// Synchronise les checkpoints locaux non-synchro vers le serveur Taijutsu.
/// Après succès (201 Created), purge les fichiers locaux.
fn cmd_sync(
    config: AnbuConfig,
    owner: String,
    repo: String,
    specific_id: Option<String>,
) -> Result<()> {
    // 1. Vérifier la configuration serveur
    let server_config = config.server.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Server not configured. Add [server] section to ~/.shinobi/config.toml:\n\n\
             [server]\n\
             url = \"http://localhost:3000\"\n\
             login = \"naruto\"\n\
             pat = \"shb_...\"\n"
        )
    })?;

    // 2. Ouvrir l'index SQLite
    let index = AnbuIndex::open(&config.database_path())?;

    // 3. Récupérer les checkpoints à synchroniser
    let checkpoints = if let Some(ref id_prefix) = specific_id {
        vec![index.find_checkpoint(id_prefix)?]
    } else {
        index.list_unsynced()?
    };

    if checkpoints.is_empty() {
        println!("\n  {} All checkpoints are already synced! 🎯\n", "✅".green());
        return Ok(());
    }

    println!();
    println!(
        "  {} Syncing {} checkpoint(s) to {}/{}",
        "🥷".bold(),
        checkpoints.len(),
        owner.cyan(),
        repo.cyan()
    );
    println!(
        "  Server: {}",
        server_config.url.dimmed()
    );
    println!();

    // 4. Créer le client HTTP
    let client = sync::AnbuSyncClient::new(server_config)?;

    // 5. Envoyer chaque checkpoint
    let mut synced_count = 0;
    let mut failed_count = 0;

    for checkpoint in &checkpoints {
        let short_id = &checkpoint.id.to_string()[..8];
        print!("  {} {short_id}...", "→".blue());

        match client.sync_checkpoint(&owner, &repo, checkpoint) {
            Ok(response) => {
                // Marquer comme synchronisé dans SQLite
                index.mark_synced(
                    &checkpoint.id.to_string(),
                    &response.id,
                )?;

                // Purge locale (Alerte Vegapunk)
                let checkpoint_dir = std::env::current_dir()?
                    .join(".shinobi")
                    .join("anbu")
                    .join("checkpoints")
                    .join(checkpoint.id.to_string());
                if let Err(e) = sync::purge_local_checkpoint(&checkpoint_dir) {
                    eprintln!(" ⚠️ purge failed: {e}");
                }

                println!(
                    " {} (IPFS: {})",
                    "✓ synced".green(),
                    &response.ipfs_cid[..12.min(response.ipfs_cid.len())].dimmed()
                );
                synced_count += 1;
            }
            Err(e) => {
                println!(" {} {e}", "✗ failed".red());
                failed_count += 1;
            }
        }
    }

    // 6. Résumé
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if failed_count == 0 {
        println!(
            "  {} {synced_count} checkpoint(s) synced to {owner}/{repo}",
            "✅".green()
        );
    } else {
        println!(
            "  {} {synced_count} synced, {} failed",
            "⚠️".yellow(),
            failed_count.to_string().red()
        );
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Formate une taille en octets en format lisible (KB, MB).
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
