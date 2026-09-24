//! Pilier 2 — `anbu jutsu logs` : Tour de contrôle terminal 🥷⚡
//!
//! Affiche les logs d'un pipeline distant dans le terminal local.
//!
//! ## Modes
//! - **One-shot** : `anbu jutsu logs <id>` — affiche l'état actuel
//! - **Follow** : `anbu jutsu logs <id> -f` — polling 2s, diff logs
//! - **Last** : `anbu jutsu logs --last -f` — dernier pipeline du repo
//!
//! ## Protocole V1 (Polling)
//! Poll les endpoints REST existants toutes les 2 secondes.
//! Diff les logs : n'affiche que les nouvelles lignes.
//! Auto-stop quand le pipeline atteint un statut terminal.
//!
//! V2 (Phase 41+) utilisera SSE pour un vrai streaming.

use std::collections::HashMap;

use anyhow::{Result, bail};
use colored::Colorize;
use serde::Deserialize;
use uuid::Uuid;

use crate::config::AnbuConfig;
use super::git_utils;

// ── API Response Types (miroir de pipeline_routes.rs) ────────────────────

#[derive(Debug, Deserialize)]
struct ApiPipelineResponse {
    id: Uuid,
    #[allow(dead_code)]
    repository_id: Uuid,
    commit_id: String,
    #[allow(dead_code)]
    trigger_event: String,
    status: String,
    pipeline_name: Option<String>,
    #[allow(dead_code)]
    started_at: Option<String>,
    #[allow(dead_code)]
    finished_at: Option<String>,
    duration_ms: Option<i32>,
    #[allow(dead_code)]
    creator_id: Option<Uuid>,
    #[allow(dead_code)]
    created_at: String,
    #[allow(dead_code)]
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct ApiStageResponse {
    #[allow(dead_code)]
    id: Uuid,
    name: String,
    image: String,
    status: String,
    #[allow(dead_code)]
    sort_order: i16,
    #[allow(dead_code)]
    started_at: Option<String>,
    #[allow(dead_code)]
    finished_at: Option<String>,
    duration_ms: Option<i32>,
    exit_code: Option<i16>,
    logs: Option<String>,
    #[allow(dead_code)]
    created_at: String,
    #[allow(dead_code)]
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct ApiPipelineDetailResponse {
    #[serde(flatten)]
    pipeline: ApiPipelineResponse,
    stages: Vec<ApiStageResponse>,
}

#[derive(Debug, Deserialize)]
struct ApiPipelineListResponse {
    pipelines: Vec<ApiPipelineResponse>,
    #[allow(dead_code)]
    total: i64,
}

// ── Handler ─────────────────────────────────────────────────────────────

/// Handler pour `anbu jutsu logs`.
pub async fn cmd_logs(
    config: AnbuConfig,
    id: Option<String>,
    follow: bool,
    last: bool,
) -> Result<()> {
    let server = config.server.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Server not configured. Run `anbu setup` first.")
    })?;

    let (owner, repo) = git_utils::resolve_owner_repo()?;
    let base_url = format!(
        "{}/api/v1/repos/{}/{}",
        server.url.trim_end_matches('/'),
        owner,
        repo
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Résoudre le pipeline_id
    let pipeline_id = if last || id.is_none() {
        // --last ou pas d'ID : prendre le dernier pipeline
        resolve_last_pipeline_id(&client, &base_url, &server.login, &server.pat)?
    } else {
        let raw_id = id.unwrap();
        // Si ce n'est pas un UUID complet (36 chars), tenter le prefix matching
        if raw_id.len() < 36 {
            resolve_pipeline_by_prefix(&client, &base_url, &server.login, &server.pat, &raw_id)?
        } else {
            raw_id
        }
    };

    println!(
        "  {} Pipeline: {}",
        "🥷".bold(),
        &pipeline_id[..8.min(pipeline_id.len())].cyan()
    );
    println!();

    if follow {
        cmd_logs_follow(&client, &base_url, &pipeline_id, &server.login, &server.pat)
    } else {
        cmd_logs_oneshot(&client, &base_url, &pipeline_id, &server.login, &server.pat)
    }
}

// ── One-shot Mode ───────────────────────────────────────────────────────

fn cmd_logs_oneshot(
    client: &reqwest::blocking::Client,
    base_url: &str,
    pipeline_id: &str,
    login: &str,
    pat: &str,
) -> Result<()> {
    let detail = fetch_pipeline_detail(client, base_url, pipeline_id, login, pat)?;
    render_pipeline(&detail, &HashMap::new(), true);
    Ok(())
}

// ── Follow Mode ─────────────────────────────────────────────────────────

fn cmd_logs_follow(
    client: &reqwest::blocking::Client,
    base_url: &str,
    pipeline_id: &str,
    login: &str,
    pat: &str,
) -> Result<()> {
    // Track last log length per stage pour le diff
    let mut last_log_len: HashMap<String, usize> = HashMap::new();

    loop {
        // Effacer l'écran (ANSI escape) pour rafraîchir l'affichage
        print!("\x1b[2J\x1b[H");

        let detail = fetch_pipeline_detail(client, base_url, pipeline_id, login, pat)?;
        render_pipeline(&detail, &last_log_len, true);

        // Mettre à jour les offsets de logs
        for stage in &detail.stages {
            let log_len = stage.logs.as_ref().map_or(0, |l| l.len());
            last_log_len.insert(stage.name.clone(), log_len);
        }

        // Vérifier si le pipeline est terminé
        let terminal_states = ["success", "failure", "error", "cancelled"];
        if terminal_states.contains(&detail.pipeline.status.as_str()) {
            println!();
            let emoji = match detail.pipeline.status.as_str() {
                "success" => "✅",
                "failure" => "❌",
                "error" => "⚠️",
                _ => "🚫",
            };
            println!(
                "  {} Pipeline finished: {} {}",
                emoji,
                detail.pipeline.status.as_str(),
                detail.pipeline.duration_ms
                    .map(|ms| format!("({}ms)", ms))
                    .unwrap_or_default()
                    .dimmed()
            );
            println!();
            break;
        }

        // Polling 2s
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Ok(())
}

// ── Rendering ───────────────────────────────────────────────────────────

fn render_pipeline(
    detail: &ApiPipelineDetailResponse,
    _last_log_len: &HashMap<String, usize>,
    show_logs: bool,
) {
    let p = &detail.pipeline;
    let name = p.pipeline_name.as_deref().unwrap_or("Pipeline");
    let short_id = &p.id.to_string()[..8];

    let status_display = format_status(&p.status);

    println!();
    println!(
        "  {}",
        "┌────────────────────────────────────────────────────────────┐".dimmed()
    );
    println!(
        "  {}  🥷 {}: {} [{}]",
        "│".dimmed(),
        "Pipeline".bold(),
        name.white().bold(),
        short_id.cyan()
    );
    println!(
        "  {}  Commit: {}   Status: {}",
        "│".dimmed(),
        p.commit_id[..12.min(p.commit_id.len())].dimmed(),
        status_display
    );
    println!(
        "  {}",
        "├────────────────────────────────────────────────────────────┤".dimmed()
    );

    for stage in &detail.stages {
        let stage_status = format_status(&stage.status);
        let duration = stage.duration_ms
            .map(|ms| {
                if ms < 1000 {
                    format!("{ms}ms")
                } else {
                    format!("{:.1}s", ms as f64 / 1000.0)
                }
            })
            .unwrap_or_default();

        println!(
            "  {}  {} {} ({}) — {} {}",
            "│".dimmed(),
            stage_icon(&stage.status),
            stage.name.white().bold(),
            stage.image.dimmed(),
            stage_status,
            duration.dimmed()
        );

        // Afficher les logs si disponibles
        if show_logs {
            if let Some(ref logs) = stage.logs {
                if !logs.is_empty() {
                    // Afficher les dernières lignes de logs (max 10 pour la lisibilité)
                    let lines: Vec<&str> = logs.lines().collect();
                    let display_lines = if lines.len() > 15 {
                        &lines[lines.len() - 15..]
                    } else {
                        &lines
                    };

                    for line in display_lines {
                        println!(
                            "  {}    {}",
                            "│".dimmed(),
                            line
                        );
                    }

                    if lines.len() > 15 {
                        println!(
                            "  {}    {} ({} lignes au total)",
                            "│".dimmed(),
                            "...".dimmed(),
                            lines.len()
                        );
                    }
                }
            }
        }

        // Exit code si failure/error
        if let Some(code) = stage.exit_code {
            if code != 0 {
                println!(
                    "  {}    {}",
                    "│".dimmed(),
                    format!("Exit code: {code}").red()
                );
            }
        }
    }

    println!(
        "  {}",
        "└────────────────────────────────────────────────────────────┘".dimmed()
    );
}

fn stage_icon(status: &str) -> String {
    match status {
        "pending" => "◯".dimmed().to_string(),
        "running" => "▶".yellow().bold().to_string(),
        "success" => "✅".to_string(),
        "failure" => "❌".to_string(),
        "error" => "⚠️".to_string(),
        "skipped" => "⏭️".to_string(),
        _ => "?".to_string(),
    }
}

fn format_status(status: &str) -> String {
    match status {
        "queued" => "queued ⏳".yellow().to_string(),
        "running" => "running ⚡".yellow().bold().to_string(),
        "success" => "success ✅".green().bold().to_string(),
        "failure" => "failure ❌".red().bold().to_string(),
        "error" => "error ⚠️".red().to_string(),
        "cancelled" => "cancelled 🚫".dimmed().to_string(),
        "pending" => "pending ◯".dimmed().to_string(),
        _ => status.to_string(),
    }
}

// ── API Helpers ─────────────────────────────────────────────────────────

fn fetch_pipeline_detail(
    client: &reqwest::blocking::Client,
    base_url: &str,
    pipeline_id: &str,
    login: &str,
    pat: &str,
) -> Result<ApiPipelineDetailResponse> {
    let url = format!("{}/pipelines/{}", base_url, pipeline_id);

    let response = client
        .get(&url)
        .basic_auth(login, Some(pat))
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to fetch pipeline: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        bail!("Server returned {status}: {body}");
    }

    let detail: ApiPipelineDetailResponse = response.json()
        .map_err(|e| anyhow::anyhow!("Failed to parse pipeline response: {e}"))?;

    Ok(detail)
}

fn resolve_last_pipeline_id(
    client: &reqwest::blocking::Client,
    base_url: &str,
    login: &str,
    pat: &str,
) -> Result<String> {
    let url = format!("{}/pipelines", base_url);

    let response = client
        .get(&url)
        .basic_auth(login, Some(pat))
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to list pipelines: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        bail!("Server returned {status}: {body}");
    }

    let list: ApiPipelineListResponse = response.json()
        .map_err(|e| anyhow::anyhow!("Failed to parse pipeline list: {e}"))?;

    let first = list.pipelines.first()
        .ok_or_else(|| anyhow::anyhow!(
            "No pipelines found for this repository.\n\
             Trigger one first: anbu jutsu trigger"
        ))?;

    Ok(first.id.to_string())
}

/// Résout un prefix de pipeline_id en UUID complet.
///
/// Cherche dans la liste des pipelines du repo celui dont l'UUID
/// commence par le prefix donné (ex: "ad8ecd28" → UUID complet).
fn resolve_pipeline_by_prefix(
    client: &reqwest::blocking::Client,
    base_url: &str,
    login: &str,
    pat: &str,
    prefix: &str,
) -> Result<String> {
    let url = format!("{}/pipelines", base_url);

    let response = client
        .get(&url)
        .basic_auth(login, Some(pat))
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to list pipelines: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        bail!("Server returned {status}: {body}");
    }

    let list: ApiPipelineListResponse = response.json()
        .map_err(|e| anyhow::anyhow!("Failed to parse pipeline list: {e}"))?;

    let prefix_lower = prefix.to_lowercase();
    let matches: Vec<&ApiPipelineResponse> = list
        .pipelines
        .iter()
        .filter(|p| p.id.to_string().starts_with(&prefix_lower))
        .collect();

    match matches.len() {
        0 => bail!(
            "No pipeline matching prefix '{prefix}'.\n\
             Use `anbu jutsu logs --last` to see the latest pipeline."
        ),
        1 => Ok(matches[0].id.to_string()),
        n => {
            let ids: Vec<String> = matches.iter().map(|p| p.id.to_string()).collect();
            bail!(
                "Ambiguous prefix '{prefix}' — matches {n} pipelines:\n  {}\n\n\
                 Use a longer prefix to narrow it down.",
                ids.join("\n  ")
            )
        }
    }
}
