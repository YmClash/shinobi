//! CLI ANBU — Définition des commandes avec clap.
//!
//! 4 commandes V1 :
//! - `checkpoint` : capture les artifacts IA et les attache au commit
//! - `log` : liste les checkpoints dans le repo courant
//! - `show` : affiche le détail d'un checkpoint
//! - `sessions` : détecte les sessions IA disponibles

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// 🥷 ANBU 暗部 — AI Context Capture for SHINOBI
///
/// Capture the "why" behind AI-generated code and attach it to Jujutsu commits.
/// Preserves implementation plans, reasoning artifacts, and conversation logs
/// so the context of creation is never lost.
#[derive(Parser, Debug)]
#[command(
    name = "anbu",
    version,
    about = "🥷 ANBU 暗部 — AI Context Capture for SHINOBI",
    long_about = "Capture the \"why\" behind AI-generated code and attach it to Jujutsu commits.\n\n\
        When AI agents (Antigravity, Cursor, Copilot) generate code, their thoughts\n\
        disappear after the session. ANBU preserves this context:\n\
        • Implementation plans and reasoning\n\
        • Task lists and progress tracking\n\
        • Walkthroughs and summaries\n\
        • Conversation logs"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Capture AI artifacts and attach to the current commit
    ///
    /// Collects artifacts from an AI agent session (implementation plans,
    /// tasks, walkthroughs, logs) and stores them locally. If running in
    /// a Jujutsu repo, attaches AI trailers to the specified revision.
    #[command(alias = "cp")]
    Checkpoint {
        /// Conversation/session ID to capture
        #[arg(short, long)]
        session: Option<String>,

        /// Auto-detect and use the most recent session
        #[arg(long, conflicts_with = "session")]
        latest: bool,

        /// Description for this checkpoint
        #[arg(short, long)]
        message: Option<String>,

        /// Additional files to attach manually
        #[arg(long, num_args = 1..)]
        attach: Vec<PathBuf>,

        /// Jujutsu revision to tag (default: @, the working copy)
        #[arg(long, default_value = "@")]
        revision: String,

        /// Skip jj trailer attachment (store locally only)
        #[arg(long)]
        no_tag: bool,
    },

    /// List checkpoints in the current repository
    #[command(alias = "l")]
    Log {
        /// Maximum number of checkpoints to display
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// Show details of a specific checkpoint
    #[command(alias = "s")]
    Show {
        /// Checkpoint ID (full UUID or short prefix)
        id: String,

        /// Display content of a specific artifact
        #[arg(long)]
        artifact: Option<String>,
    },

    /// List detected AI agent sessions
    Sessions {
        /// Maximum number of sessions to display
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Filter by agent type
        #[arg(long)]
        agent: Option<String>,
    },
}
