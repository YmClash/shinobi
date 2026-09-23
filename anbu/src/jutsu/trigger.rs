//! Pilier 1 — `anbu jutsu trigger` : Déclencheur distant 🥷⚡
//!
//! Force l'exécution d'un pipeline sur le serveur SHINOBI sans
//! polluer l'historique Git (pas de commit factice).
//!
//! ## Flux
//! 1. Résout `owner/repo` depuis git remote (ou args CLI)
//! 2. Résout `commit_id` depuis HEAD (ou `--ref`)
//! 3. `POST /api/v1/repos/{owner}/{repo}/pipelines/trigger`
//! 4. Affiche le pipeline_id + lien dashboard
//!
//! ## Authentification
//! Basic Auth via PAT stocké dans `~/.shinobi/config.toml → [server]`

use anyhow::{Result, bail};
use colored::Colorize;

use crate::config::AnbuConfig;
use super::git_utils;

/// Handler pour `anbu jutsu trigger`.
pub async fn cmd_trigger(
    config: AnbuConfig,
    ref_name: Option<String>,
    owner_arg: Option<String>,
    repo_arg: Option<String>,
) -> Result<()> {
    // 1. Config serveur obligatoire
    let server = config.server.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Server not configured. Run `anbu setup` first.\n\
             Or add [server] section to ~/.shinobi/config.toml"
        )
    })?;

    // 2. Résoudre owner/repo
    let (owner, repo) = match (owner_arg, repo_arg) {
        (Some(o), Some(r)) => (o, r),
        _ => {
            git_utils::resolve_owner_repo().map_err(|e| {
                anyhow::anyhow!(
                    "{e}\n\nTip: Use --owner and --repo to specify manually:\n  \
                     anbu jutsu trigger --owner naruto --repo boruto"
                )
            })?
        }
    };

    // 3. Résoudre commit_id
    let commit_id = git_utils::resolve_commit_id()?;

    println!();
    println!(
        "  {} Triggering pipeline...",
        "🥷".bold()
    );
    println!(
        "  Repository: {}/{}",
        owner.cyan(),
        repo.cyan()
    );
    println!(
        "  Commit:     {}",
        commit_id[..12.min(commit_id.len())].dimmed()
    );

    // 4. POST /api/v1/repos/{owner}/{repo}/pipelines/trigger
    let url = format!(
        "{}/api/v1/repos/{}/{}/pipelines/trigger",
        server.url.trim_end_matches('/'),
        owner,
        repo
    );

    let body = serde_json::json!({
        "commit_id": commit_id,
        "ref_name": ref_name,
    });

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .basic_auth(&server.login, Some(&server.pat))
        .json(&body)
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to connect to server: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().unwrap_or_default();
        bail!(
            "Server returned {status}: {body_text}\n\n\
             Check that:\n  \
             • The server is running at {}\n  \
             • Your PAT is valid (anbu setup)\n  \
             • The repo {}/{} has a jutsu.yml",
            server.url,
            owner,
            repo
        );
    }

    let result: serde_json::Value = response.json()
        .map_err(|e| anyhow::anyhow!("Failed to parse server response: {e}"))?;

    let pipeline_id = result["pipeline_id"]
        .as_str()
        .unwrap_or("unknown");
    let pipeline_status = result["status"]
        .as_str()
        .unwrap_or("queued");

    // 5. Affichage résultat
    println!();
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!(
        "  {} Pipeline triggered!",
        "⚡".green()
    );
    println!(
        "  Pipeline:  {}",
        &pipeline_id[..8.min(pipeline_id.len())].cyan().bold()
    );
    println!(
        "  Commit:    {}",
        &commit_id[..12.min(commit_id.len())].dimmed()
    );
    println!(
        "  Status:    {} ⏳",
        pipeline_status.yellow()
    );
    println!();
    println!(
        "  Dashboard: {}/repos/{}/{}/pipelines",
        server.url.trim_end_matches('/'),
        owner,
        repo
    );
    println!(
        "  Logs:      {}",
        format!("anbu jutsu logs {} -f", &pipeline_id[..8.min(pipeline_id.len())])
            .cyan()
    );
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!();

    Ok(())
}
