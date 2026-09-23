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
    pub command: Option<Commands>,
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

        /// Filter by agent type (antigravity, copilot)
        #[arg(long)]
        agent: Option<String>,

        /// Show sessions from all VS Code workspaces (Copilot)
        #[arg(long)]
        all_workspaces: bool,
    },

    /// Sync local checkpoints to the SHINOBI server (IPFS + PostgreSQL)
    ///
    /// Reads artifacts from .shinobi/anbu/checkpoints/ and uploads them
    /// to the server. After successful upload, local files are purged
    /// (the SQLite index retains the metadata).
    #[command(alias = "push")]
    Sync {
        /// Owner of the target repository (ex: "naruto")
        #[arg(long)]
        owner: String,

        /// Repository name (ex: "boruto")
        #[arg(long)]
        repo: String,

        /// Sync only a specific checkpoint (default: all unsynced)
        #[arg(long)]
        id: Option<String>,
    },

    /// Configure ANBU server connection (interactive wizard)
    ///
    /// Sets up the connection to your SHINOBI server by configuring
    /// the server URL, login, and Personal Access Token (PAT).
    /// By default, launches an interactive wizard. Use flags for
    /// silent/scripted configuration.
    Setup {
        /// Server URL (ex: "http://localhost:3000")
        #[arg(long)]
        server_url: Option<String>,

        /// Login username
        #[arg(long)]
        login: Option<String>,

        /// Personal Access Token (will be masked in interactive mode)
        #[arg(long)]
        pat: Option<String>,
    },

    /// 🥷⚡ Jutsu Runner — CI/CD Pipeline operations
    ///
    /// Trigger, monitor, and run CI/CD pipelines from the terminal.
    /// Supports remote server pipelines and local Docker execution.
    #[command(alias = "j")]
    Jutsu {
        #[command(subcommand)]
        action: JutsuCommands,
    },
}

/// Sub-commands for `anbu jutsu`.
#[derive(Subcommand, Debug)]
pub enum JutsuCommands {
    /// Trigger a remote pipeline execution on the server
    ///
    /// Forces pipeline execution on the SHINOBI server without polluting
    /// the Git history. Resolves owner/repo from git remote and commit
    /// from HEAD automatically.
    #[command(alias = "t")]
    Trigger {
        /// Git ref to build (branch or tag, default: current HEAD)
        #[arg(long)]
        r#ref: Option<String>,

        /// Repository owner (auto-detected from git remote)
        #[arg(long)]
        owner: Option<String>,

        /// Repository name (auto-detected from git remote)
        #[arg(long)]
        repo: Option<String>,
    },

    /// Stream pipeline logs from the server
    ///
    /// Displays logs for a specific pipeline or the most recent one.
    /// In follow mode (-f), polls every 2 seconds and shows only new lines.
    #[command(alias = "l")]
    Logs {
        /// Pipeline ID (full UUID or short prefix)
        id: Option<String>,

        /// Follow mode — poll for new logs every 2s until pipeline finishes
        #[arg(short, long)]
        follow: bool,

        /// Show logs for the latest pipeline of this repo
        #[arg(long)]
        last: bool,
    },

    /// Run pipeline locally using Docker
    ///
    /// Reads jutsu.yml from the current directory and executes each stage
    /// in a local Docker container. The current directory is mounted as
    /// the workspace — no git clone needed.
    #[command(alias = "r")]
    Run {
        /// Execute locally (reads ./jutsu.yml, runs via Docker)
        #[arg(long)]
        local: bool,

        /// Run only a specific stage
        #[arg(long)]
        stage: Option<String>,

        /// Validate YAML without executing (parse + cycle check only)
        #[arg(long)]
        check: bool,

        /// Path to jutsu.yml (default: ./jutsu.yml)
        #[arg(long)]
        file: Option<String>,
    },
}

