//! Utilitaires Git — Résolution owner/repo/commit depuis le repo local.
//!
//! Phase 40-E : Utilisé par `anbu jutsu trigger` et `anbu jutsu logs`
//! pour auto-détecter le dépôt et le commit courant.
//!
//! ## Stratégie de résolution
//! 1. `owner/repo` : parse `git config remote.origin.url`
//!    - SSH : `git@host:owner/repo.git` → `(owner, repo)`
//!    - HTTPS : `https://host/owner/repo.git` → `(owner, repo)`
//! 2. `commit_id` : `git rev-parse HEAD`
//!    - Fonctionne aussi en mode jj colocalisé (Phase 12 — GitBackend)

use std::process::Command;

use anyhow::{Context, Result, bail};

/// Résout `(owner, repo)` depuis le remote `origin` du repo Git courant.
///
/// Supporte les formats :
/// - SSH : `git@github.com:owner/repo.git`
/// - HTTPS : `https://github.com/owner/repo.git`
/// - Local Shinobi : `http://localhost:3000/owner/repo.git`
pub fn resolve_owner_repo() -> Result<(String, String)> {
    let output = Command::new("git")
        .args(["config", "remote.origin.url"])
        .output()
        .context("Failed to run 'git config remote.origin.url' — are you in a git repo?")?;

    if !output.status.success() {
        bail!(
            "No git remote 'origin' found.\n\
             Use --owner and --repo to specify manually."
        );
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_remote_url(&url)
}

/// Parse une URL de remote Git pour en extraire `(owner, repo)`.
///
/// ## Formats supportés
/// - `git@host:owner/repo.git` → SSH
/// - `https://host/owner/repo.git` → HTTPS
/// - `http://host/owner/repo.git` → HTTP
/// - `http://host/owner/repo` → sans `.git` suffix
fn parse_remote_url(url: &str) -> Result<(String, String)> {
    let path = if let Some(rest) = url.strip_prefix("git@") {
        // SSH format: git@host:owner/repo.git
        rest.split_once(':')
            .map(|(_, path)| path)
            .ok_or_else(|| anyhow::anyhow!("Invalid SSH remote URL: {url}"))?
    } else if url.starts_with("https://") || url.starts_with("http://") {
        // HTTPS/HTTP format: https://host/owner/repo.git
        // Skip the first 3 segments (scheme + empty + host)
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() < 5 {
            bail!("Invalid HTTP remote URL: {url}");
        }
        // Reconstruct "owner/repo.git" from the last two segments
        // Handle paths like /owner/repo.git or /prefix/owner/repo.git
        let len = parts.len();
        // Take last two path segments
        &url[url.rfind(&format!("/{}/", parts[len - 2])).unwrap_or(0) + 1..]
    } else {
        bail!("Unsupported remote URL format: {url}");
    };

    // Parse "owner/repo.git" or "owner/repo"
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("Cannot extract owner/repo from: {path}"))?;

    if owner.is_empty() || repo.is_empty() {
        bail!("Empty owner or repo in remote URL: {url}");
    }

    Ok((owner.to_string(), repo.to_string()))
}

/// Résout le SHA du commit HEAD courant via `git rev-parse HEAD`.
///
/// Fonctionne aussi en mode jj colocalisé car jj maintient
/// un répertoire `.git/` synchronisé (Phase 12 — GitBackend).
pub fn resolve_commit_id() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .context("Failed to run 'git rev-parse HEAD'")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rev-parse HEAD failed: {stderr}");
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        bail!("git rev-parse HEAD returned empty — is this a fresh repo with no commits?");
    }

    Ok(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_url() {
        let (owner, repo) = parse_remote_url("git@github.com:naruto/boruto.git").unwrap();
        assert_eq!(owner, "naruto");
        assert_eq!(repo, "boruto");
    }

    #[test]
    fn test_parse_https_url() {
        let (owner, repo) = parse_remote_url("https://github.com/naruto/boruto.git").unwrap();
        assert_eq!(owner, "naruto");
        assert_eq!(repo, "boruto");
    }

    #[test]
    fn test_parse_http_localhost() {
        let (owner, repo) = parse_remote_url("http://localhost:3000/naruto/boruto.git").unwrap();
        assert_eq!(owner, "naruto");
        assert_eq!(repo, "boruto");
    }

    #[test]
    fn test_parse_url_without_git_suffix() {
        let (owner, repo) = parse_remote_url("https://github.com/naruto/boruto").unwrap();
        assert_eq!(owner, "naruto");
        assert_eq!(repo, "boruto");
    }

    #[test]
    fn test_parse_invalid_url() {
        assert!(parse_remote_url("not-a-url").is_err());
    }
}
