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
mod setup;
mod storage;
mod sync;
mod vcs;

use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::cli::{Cli, Commands};
use crate::collectors::antigravity::AntigravityCollector;
use crate::collectors::copilot::CopilotCollector;
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
        Some(Commands::Sessions { limit, agent, all_workspaces }) => cmd_sessions(config, limit, agent, all_workspaces),
        Some(Commands::Sync { owner, repo, id }) => cmd_sync(config, owner, repo, id),
        Some(Commands::Setup { server_url, login, pat }) => cmd_setup(server_url, login, pat),
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
            println!("    {}        Sync checkpoints to server", "sync".cyan());
            println!("    {}       Configure server connection", "setup".cyan());
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
    // Résoudre la session + agent source
    let (session_id, agent_kind, collected) = if let Some(sid) = session {
        // Session explicite → tenter Antigravity d'abord, puis Copilot
        let ag = AntigravityCollector::new(config.brain_path());
        if let Ok(arts) = ag.collect_session(&sid) {
            (sid, models::AgentKind::Antigravity, arts)
        } else {
            let cp = CopilotCollector::new(false);
            let arts = cp.collect_session(&sid)?;
            (sid, models::AgentKind::Copilot, arts)
        }
    } else if latest {
        // --latest : scanner les deux collecteurs, prendre le plus récent
        resolve_latest_session(&config)?
    } else if !attach.is_empty() {
        // Mode --attach sans session : crée un checkpoint manuel
        ("manual".to_string(), models::AgentKind::Antigravity, Vec::new())
    } else {
        anyhow::bail!(
            "Specify --session <id> or --latest to select a session.\n\
             Use `anbu sessions` to list available sessions."
        );
    };

    println!(
        "{} Capturing session {} [{}]...",
        "🥷".bold(),
        session_id[..8.min(session_id.len())].cyan(),
        agent_kind.to_string().magenta()
    );

    // Ajouter les fichiers manuels --attach
    let mut collected = collected;
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
        agent_kind,
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
            Ok(new_commit_id) => {
                println!(
                    "  {} Trailers attached to revision {}",
                    "✓".green(),
                    revision.cyan()
                );
                // Ceinture-Bretelles: synchroniser le commit_id muté dans SQLite
                if let Err(e) = index.update_commit_id(
                    &checkpoint.id.to_string(),
                    &new_commit_id,
                ) {
                    eprintln!(
                        "  {} Failed to update commit_id in index: {e}",
                        "⚠".yellow()
                    );
                } else {
                    println!(
                        "  {} Commit ID synced: {}",
                        "✓".green(),
                        new_commit_id[..12.min(new_commit_id.len())].dimmed()
                    );
                }
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

fn cmd_sessions(config: AnbuConfig, limit: usize, agent_filter: Option<String>, all_workspaces: bool) -> Result<()> {
    let mut all_sessions = Vec::new();

    // Filtrer par agent si spécifié
    let show_antigravity = agent_filter.as_ref().map_or(true, |a| {
        a.eq_ignore_ascii_case("antigravity") || a.eq_ignore_ascii_case("gemini")
    });
    let show_copilot = agent_filter.as_ref().map_or(true, |a| {
        a.eq_ignore_ascii_case("copilot") || a.eq_ignore_ascii_case("github-copilot")
    });

    // Scanner Antigravity
    if show_antigravity {
        let collector = AntigravityCollector::new(config.brain_path());
        match collector.detect_sessions() {
            Ok(sessions) => all_sessions.extend(sessions),
            Err(e) => eprintln!("  {} Antigravity scan: {e}", "⚠".yellow()),
        }
    }

    // Scanner Copilot
    if show_copilot {
        let collector = CopilotCollector::new(all_workspaces);
        match collector.detect_sessions() {
            Ok(sessions) => all_sessions.extend(sessions),
            Err(e) => eprintln!("  {} Copilot scan: {e}", "⚠".yellow()),
        }
    }

    // Trier par date décroissante (toutes sources confondues)
    all_sessions.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

    if all_sessions.is_empty() {
        println!(
            "  {} No AI sessions detected",
            "ℹ".blue(),
        );
        if show_antigravity {
            println!("    Antigravity brain: {}", config.brain_path().display());
        }
        if show_copilot {
            println!("    Copilot: VS Code workspaceStorage");
        }
        return Ok(());
    }

    let display_count = limit.min(all_sessions.len());

    println!();
    println!(
        " {} {} session(s) detected (showing {})",
        "🧠 AI Sessions".bold(),
        all_sessions.len(),
        display_count
    );
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );

    for session in all_sessions.iter().take(display_count) {
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
            "  {}  {}  {} artifact(s)  {}  {}",
            short_id.cyan().bold(),
            date.to_string().dimmed(),
            session.artifact_count.to_string().green(),
            session.agent.to_string().magenta(),
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

// ── Smart --latest Resolution ─────────────────────────────────────────────

/// Résout la session la plus récente parmi tous les collecteurs.
///
/// ## Logique du "Duel Final"
///
/// 1. Scanne Antigravity (brain/) → prend la session la plus récente
/// 2. Scanne Copilot (workspaceStorage/) filtré au **workspace courant** uniquement
/// 3. Compare les timestamps et prend la gagnante
/// 4. Collecte les artifacts de la session gagnante
///
/// Retourne `(session_id, agent_kind, collected_artifacts)`.
fn resolve_latest_session(
    config: &AnbuConfig,
) -> Result<(String, models::AgentKind, Vec<collectors::CollectedArtifact>)> {
    let mut candidates: Vec<models::SessionInfo> = Vec::new();

    // 1. Antigravity sessions
    let ag = AntigravityCollector::new(config.brain_path());
    match ag.detect_sessions() {
        Ok(sessions) => {
            if let Some(latest) = sessions.into_iter().next() {
                candidates.push(latest);
            }
        }
        Err(e) => eprintln!("  {} Antigravity scan: {e}", "⚠".yellow()),
    }

    // 2. Copilot sessions — filtré au workspace courant (all_workspaces = false)
    let cp = CopilotCollector::new(false);
    match cp.detect_sessions() {
        Ok(sessions) => {
            if let Some(latest) = sessions.into_iter().next() {
                candidates.push(latest);
            }
        }
        Err(e) => eprintln!("  {} Copilot scan: {e}", "⚠".yellow()),
    }

    if candidates.is_empty() {
        anyhow::bail!(
            "No AI sessions detected.\n\
             • Antigravity brain: {}\n\
             • Copilot: current workspace only\n\
             Use `anbu sessions` to list all sessions.",
            config.brain_path().display()
        );
    }

    // 3. Le Duel Final — la session la plus récente gagne
    candidates.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    let winner = &candidates[0];

    let winner_id = winner.id.clone();
    let winner_agent = winner.agent.clone();

    println!(
        "  {} Latest session: {} [{}] ({})",
        "→".blue(),
        winner_id[..8.min(winner_id.len())].cyan(),
        winner_agent.to_string().magenta(),
        winner.last_modified.format("%Y-%m-%d %H:%M").to_string().dimmed()
    );

    // 4. Collecter les artifacts de la gagnante
    let collected = match winner_agent {
        models::AgentKind::Antigravity => {
            ag.collect_session(&winner_id)?
        }
        models::AgentKind::Copilot => {
            cp.collect_session(&winner_id)?
        }
    };

    Ok((winner_id, winner_agent, collected))
}

// ── Setup Command ─────────────────────────────────────────────────────────

/// Handler: `anbu setup`
///
/// Lance le wizard de configuration serveur (interactif ou silencieux).
fn cmd_setup(
    server_url: Option<String>,
    login: Option<String>,
    pat: Option<String>,
) -> Result<()> {
    setup::run_setup(server_url, login, pat)
}

