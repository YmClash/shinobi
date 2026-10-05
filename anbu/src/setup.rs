//! Module setup — Wizard de configuration ANBU.
//!
//! Commande `anbu setup` : configure la connexion au serveur SHINOBI.
//!
//! ## Modes
//! - **Interactif** (défaut) : prompts via `dialoguer` (masquage PAT)
//! - **Silencieux** : `anbu setup --server-url X --login Y --pat Z`
//!
//! ## Flux
//! 1. Demande l'URL du serveur (défaut : `http://localhost:3000`)
//! 2. Demande le login
//! 3. Demande le PAT (masqué à l'écran)
//! 4. Test de connexion via `GET /health`
//! 5. Écriture dans `~/.shinobi/config.toml`

use anyhow::{Context, Result, bail};
use colored::Colorize;
use dialoguer::{Confirm, Input, Password};

use crate::config::{AnbuConfig, ServerSection};

/// Exécute le wizard de configuration.
///
/// Si tous les arguments sont fournis (`server_url`, `login`, `pat`),
/// le mode silencieux est utilisé — aucun prompt interactif.
pub fn run_setup(
    server_url: Option<String>,
    login: Option<String>,
    pat: Option<String>,
) -> Result<()> {
    println!();
    println!(
        "  {} {}",
        "🥷 ANBU Setup".bold(),
        "— Configuration du serveur SHINOBI".dimmed()
    );
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!();

    // Charger la config existante
    let mut config = AnbuConfig::load()?;

    // Vérifier si une config serveur existe déjà
    if config.server.is_some() {
        let overwrite = if server_url.is_some() && login.is_some() && pat.is_some() {
            // Mode silencieux : écraser sans demander
            true
        } else {
            Confirm::new()
                .with_prompt("  ⚠ Server configuration already exists. Overwrite?")
                .default(false)
                .interact()
                .unwrap_or(false)
        };

        if !overwrite {
            println!("  {} Setup cancelled.", "ℹ".blue());
            return Ok(());
        }
    }

    // Déterminer le mode (interactif ou silencieux) avant de consommer les Options
    let is_interactive = server_url.is_none();

    // Résoudre les valeurs (interactif ou silencieux)
    let resolved_url = match server_url {
        Some(url) => url,
        None => Input::new()
            .with_prompt("  Server URL")
            .default("http://localhost:3000".to_string())
            .interact_text()
            .context("Failed to read server URL")?,
    };

    let resolved_login = match login {
        Some(l) => l,
        None => Input::new()
            .with_prompt("  Login")
            .interact_text()
            .context("Failed to read login")?,
    };

    let resolved_pat = match pat {
        Some(p) => p,
        None => Password::new()
            .with_prompt("  Personal Access Token (PAT)")
            .interact()
            .context("Failed to read PAT")?,
    };

    // Valider les entrées
    if resolved_url.is_empty() {
        bail!("Server URL cannot be empty");
    }
    if resolved_login.is_empty() {
        bail!("Login cannot be empty");
    }
    if resolved_pat.is_empty() {
        bail!("PAT cannot be empty");
    }

    // Test de connexion
    println!();
    print!(
        "  {} Testing connection to {}...",
        "→".blue(),
        resolved_url.cyan()
    );

    match test_connection(&resolved_url) {
        Ok(()) => {
            println!(" {}", "✓ connected".green());
        }
        Err(e) => {
            println!(" {}", "✗ failed".red());
            eprintln!("    {e}");
            eprintln!();

            // En mode interactif, demander si on veut quand même sauvegarder
            let save_anyway = if is_interactive {
                Confirm::new()
                    .with_prompt("  Save configuration anyway?")
                    .default(false)
                    .interact()
                    .unwrap_or(false)
            } else {
                false
            };

            if !save_anyway {
                bail!("Connection test failed. Configuration not saved.");
            }
        }
    }

    // Sauvegarder la configuration
    config.server = Some(ServerSection {
        url: resolved_url.clone(),
        login: resolved_login.clone(),
        pat: resolved_pat,
    });
    config.save()?;

    println!();
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!(
        "  {} Configuration saved to {}",
        "✅".green(),
        "~/.shinobi/config.toml".cyan()
    );
    println!(
        "  Server: {} │ Login: {}",
        resolved_url.dimmed(),
        resolved_login.green()
    );
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!();
    println!(
        "  You can now sync checkpoints: {}",
        "anbu sync --owner <owner> --repo <repo>".cyan()
    );
    println!();

    Ok(())
}

/// Teste la connexion au serveur SHINOBI via `GET /health`.
///
/// Retourne Ok(()) si le serveur répond avec un status 200.
fn test_connection(server_url: &str) -> Result<()> {
    let url = format!("{}/health", server_url.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("Failed to create HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("Cannot reach server at {url}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        bail!(
            "Server returned HTTP {} — check the URL",
            response.status()
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_setup_module_exists() {
        // Ce test vérifie que le module compile correctement.
        // Les tests fonctionnels nécessitent un serveur en cours d'exécution.
        assert!(true);
    }
}
