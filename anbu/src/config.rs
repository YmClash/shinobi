//! Configuration ANBU — Résolution des chemins et préférences utilisateur.
//!
//! La configuration est stockée dans `~/.shinobi/config.toml` et créée
//! automatiquement au premier lancement.
//!
//! ## Chemins
//! - `~/.shinobi/config.toml` : configuration globale
//! - `~/.shinobi/anbu.db` : index SQLite (WAL mode)
//! - `~/.gemini/antigravity/brain/` : source des artifacts Antigravity

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Configuration globale ANBU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnbuConfig {
    /// Section ANBU.
    #[serde(default)]
    pub anbu: AnbuSection,
}

/// Section `[anbu]` de la configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnbuSection {
    /// Chemin vers le brain Antigravity/Gemini.
    /// Défaut : `~/.gemini/antigravity/brain`
    pub antigravity_brain_path: Option<String>,

    /// Chemin vers la base SQLite.
    /// Défaut : `~/.shinobi/anbu.db`
    pub database_path: Option<String>,

    /// Attacher automatiquement les trailers jj aux commits ?
    #[serde(default = "default_true")]
    pub auto_trailers: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AnbuSection {
    fn default() -> Self {
        Self {
            antigravity_brain_path: None,
            database_path: None,
            auto_trailers: true,
        }
    }
}

impl AnbuConfig {
    /// Charge la configuration depuis `~/.shinobi/config.toml`.
    /// Crée le fichier et le répertoire s'ils n'existent pas.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_file_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config: {}", config_path.display()))?;
            let config: AnbuConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config: {}", config_path.display()))?;
            Ok(config)
        } else {
            // Créer le répertoire et le fichier par défaut
            let config = AnbuConfig::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Sauvegarde la configuration dans `~/.shinobi/config.toml`.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_file_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config dir: {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config: {}", config_path.display()))?;
        Ok(())
    }

    /// Résout le chemin du brain Antigravity.
    pub fn brain_path(&self) -> PathBuf {
        if let Some(ref custom) = self.anbu.antigravity_brain_path {
            PathBuf::from(shellexpand(custom))
        } else {
            // Défaut : ~/.gemini/antigravity/brain
            home_dir()
                .join(".gemini")
                .join("antigravity")
                .join("brain")
        }
    }

    /// Résout le chemin de la base SQLite.
    pub fn database_path(&self) -> PathBuf {
        if let Some(ref custom) = self.anbu.database_path {
            PathBuf::from(shellexpand(custom))
        } else {
            // Défaut : ~/.shinobi/anbu.db
            shinobi_dir().join("anbu.db")
        }
    }

    /// Chemin du fichier de configuration.
    fn config_file_path() -> Result<PathBuf> {
        Ok(shinobi_dir().join("config.toml"))
    }
}

impl Default for AnbuConfig {
    fn default() -> Self {
        Self {
            anbu: AnbuSection::default(),
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Résout le répertoire home de l'utilisateur.
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Résout le répertoire `~/.shinobi/` (configuration globale ANBU).
pub fn shinobi_dir() -> PathBuf {
    home_dir().join(".shinobi")
}

/// Expansion simplifiée de `~` en chemin absolu.
fn shellexpand(path: &str) -> String {
    if path.starts_with("~/") || path.starts_with("~\\") {
        let home = home_dir();
        format!("{}{}", home.display(), &path[1..])
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AnbuConfig::default();
        assert!(config.anbu.auto_trailers);
        assert!(config.anbu.antigravity_brain_path.is_none());
    }

    #[test]
    fn test_brain_path_default() {
        let config = AnbuConfig::default();
        let path = config.brain_path();
        assert!(path.ends_with("brain"));
    }

    #[test]
    fn test_shellexpand_tilde() {
        let expanded = shellexpand("~/Documents/test");
        assert!(!expanded.starts_with("~"));
        assert!(expanded.contains("Documents"));
    }

    #[test]
    fn test_shellexpand_no_tilde() {
        let expanded = shellexpand("/absolute/path");
        assert_eq!(expanded, "/absolute/path");
    }
}
