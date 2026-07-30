//! Intégration Jujutsu — Attache les trailers AI aux commits via `jj describe`.
//!
//! Utilise `jj` en subprocess (pas jj-lib) pour rester compatible avec
//! toute version de jj installée et éviter la complexité `!Send/!Sync`.
//!
//! ## Trailers ajoutés
//! | Trailer | Valeur | Usage |
//! |---------|--------|-------|
//! | `AI-Agent` | `antigravity` | Identifie l'agent source |
//! | `AI-Session` | `71c772c1-...` | Conversation ID |
//! | `AI-Checkpoint` | `abc123-...` | UUID du checkpoint ANBU |
//!
//! ## Anti-duplication (safeguard Vegapunk)
//! Si un trailer `AI-Checkpoint` existe déjà sur le commit, il est
//! remplacé (pas dupliqué). Cela permet de relancer `anbu checkpoint`
//! sur le même commit sans empiler les trailers.

use std::process::Command;

use anyhow::{Context, Result};

use crate::models::Checkpoint;

/// Attache les trailers AI au commit spécifié via `jj describe`.
///
/// ## Safeguard B (Vegapunk) : Output JSON
/// Utilise `-T` avec un template pour l'extraction de la description,
/// ce qui est plus robuste que le parsing de texte brut.
///
/// ## Safeguard anti-duplication
/// Parse la description existante et remplace les trailers AI-* existants.
pub fn attach_trailers(revision: &str, checkpoint: &Checkpoint) -> Result<()> {
    // Vérifier que jj est disponible
    check_jj_available()?;

    // 1. Lire la description actuelle (safeguard B: format structuré)
    let current_desc = read_description(revision)?;

    // 2. Supprimer les anciens trailers AI-* (anti-duplication)
    let clean_desc = strip_ai_trailers(&current_desc);

    // 3. Construire la nouvelle description avec trailers
    let trailers = format!(
        "AI-Agent: {}\nAI-Session: {}\nAI-Checkpoint: {}",
        checkpoint.agent,
        checkpoint.session_id,
        checkpoint.id
    );

    let new_desc = if clean_desc.trim().is_empty() {
        // Pas de description existante — juste les trailers
        trailers
    } else {
        format!("{}\n\n{}", clean_desc.trim_end(), trailers)
    };

    // 4. Appliquer via jj describe
    let output = Command::new("jj")
        .args(["describe", revision, "-m", &new_desc])
        .output()
        .context("Failed to execute `jj describe`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jj describe failed: {stderr}");
    }

    Ok(())
}

/// Lit la description actuelle d'un commit jj.
///
/// Utilise `-T description` pour extraire uniquement la description
/// sans formatage superflu.
fn read_description(revision: &str) -> Result<String> {
    let output = Command::new("jj")
        .args(["log", "--no-graph", "-r", revision, "-T", "description"])
        .output()
        .context("Failed to execute `jj log`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jj log failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Supprime tous les trailers AI-* d'une description de commit.
///
/// Parse la description ligne par ligne et retire les lignes
/// qui commencent par `AI-Agent:`, `AI-Session:`, `AI-Checkpoint:`.
/// Supprime aussi les lignes vides résiduelles en fin de description.
fn strip_ai_trailers(desc: &str) -> String {
    let lines: Vec<&str> = desc
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("AI-Agent:")
                && !trimmed.starts_with("AI-Session:")
                && !trimmed.starts_with("AI-Checkpoint:")
        })
        .collect();

    // Trim les lignes vides en fin
    let mut result: Vec<&str> = lines;
    while result.last().map_or(false, |l| l.trim().is_empty()) {
        result.pop();
    }

    result.join("\n")
}

/// Vérifie que `jj` est installé et accessible.
fn check_jj_available() -> Result<()> {
    match Command::new("jj").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => anyhow::bail!(
            "jj is installed but returned an error.\n\
             Make sure Jujutsu is properly configured."
        ),
        Err(_) => anyhow::bail!(
            "jj is not installed or not in PATH.\n\
             Install it from https://github.com/jj-vcs/jj"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ai_trailers_clean() {
        let desc = "feat: add new feature\n\nSome body text";
        let result = strip_ai_trailers(desc);
        assert_eq!(result, "feat: add new feature\n\nSome body text");
    }

    #[test]
    fn test_strip_ai_trailers_with_existing() {
        let desc = "feat: add new feature\n\nAI-Agent: antigravity\nAI-Session: abc123\nAI-Checkpoint: def456";
        let result = strip_ai_trailers(desc);
        assert_eq!(result, "feat: add new feature");
    }

    #[test]
    fn test_strip_ai_trailers_mixed() {
        let desc = "feat: refactor\n\nSigned-off-by: user\nAI-Agent: antigravity\nAI-Checkpoint: old-id\nCo-authored-by: bot";
        let result = strip_ai_trailers(desc);
        assert_eq!(result, "feat: refactor\n\nSigned-off-by: user\nCo-authored-by: bot");
    }

    #[test]
    fn test_strip_ai_trailers_empty() {
        let desc = "AI-Agent: antigravity\nAI-Session: abc";
        let result = strip_ai_trailers(desc);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_preserves_trailing_newlines_removal() {
        let desc = "message\n\n\nAI-Agent: x\n\n";
        let result = strip_ai_trailers(desc);
        assert_eq!(result, "message");
    }
}
