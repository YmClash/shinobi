//! Module Jutsu — CI/CD Pipeline operations for ANBU 🥷⚡
//!
//! Phase 40-E : Intègre les capacités Jutsu Runner dans le CLI ANBU.
//!
//! ## Piliers
//! - `trigger` : Déclenche un pipeline distant via API REST
//! - `logs`    : Surveille les logs d'un pipeline en temps réel
//! - `runner`  : Exécute un pipeline localement via Docker
//!
//! ## Architecture
//! ```text
//! cli.rs → JutsuCommands::Trigger → trigger.rs → POST /pipelines/trigger
//!        → JutsuCommands::Logs    → logs.rs    → GET  /pipelines/{id}/stages (polling)
//!        → JutsuCommands::Run     → runner.rs  → bollard (Docker local)
//! ```

pub mod config;
pub mod git_utils;
pub mod logs;
pub mod parser;
pub mod runner;
pub mod trigger;

use anyhow::Result;

use crate::cli::JutsuCommands;
use crate::config::AnbuConfig;

/// Route les sous-commandes `anbu jutsu <action>` vers les handlers.
pub async fn handle_jutsu(config: AnbuConfig, action: JutsuCommands) -> Result<()> {
    match action {
        JutsuCommands::Trigger { r#ref, owner, repo } => {
            trigger::cmd_trigger(config, r#ref, owner, repo).await
        }
        JutsuCommands::Logs { id, follow, last } => {
            logs::cmd_logs(config, id, follow, last).await
        }
        JutsuCommands::Run { local, stage, check, file } => {
            runner::cmd_run(config, local, stage, check, file).await
        }
    }
}
