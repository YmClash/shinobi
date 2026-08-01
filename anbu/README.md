# 🥷 ANBU 暗部 — AI Context Capture CLI

> *"The code tells you how. The AI context tells you why."*

**ANBU** (暗部 — *dark division*) is a standalone CLI tool that captures, indexes, and synchronizes AI-generated context alongside your code. When AI agents like Antigravity (Gemini) or GitHub Copilot write code, their reasoning—implementation plans, walkthroughs, conversation logs—disappears after the session. ANBU preserves it.

Part of the [SHINOBI](https://github.com/YmClash/shinobi) ecosystem.

---

## ✨ Features

- **🔍 Multi-Agent Detection** — Automatically scans Antigravity (Gemini) and GitHub Copilot sessions
- **📦 Artifact Capture** — Collects plans, tasks, walkthroughs, logs, screenshots, and any file
- **🏷️ Jujutsu Integration** — Attaches `AI-Agent`, `AI-Session`, `AI-Checkpoint` trailers to commits
- **☁️ Server Sync** — Uploads checkpoints to Taijutsu (IPFS + PostgreSQL) via streaming multipart
- **🗄️ Local Index** — SQLite database with WAL mode for fast, concurrent access
- **🧹 Auto-Purge** — Local artifacts are cleaned after successful server sync
- **🔒 .gitignore Safety** — Auto-injects ignore rules to prevent accidental versioning

---

## 📥 Installation

### From Source (Cargo)

```bash
cd anbu/
cargo install --path .
```

The binary `anbu` will be installed in `~/.cargo/bin/`.

### Verify Installation

```bash
anbu --version
# 🥷 ANBU 暗部 v0.1.0
```

---

## 🚀 Quick Start

```bash
# 1. See available AI sessions
anbu sessions

# 2. Capture the latest session and tag the current commit
anbu checkpoint --latest -m "Phase 28: IPFS integration"

# 3. Configure the server connection
anbu setup

# 4. Sync to the SHINOBI server
anbu sync --owner YmClash --repo boruto
```

---

## 📖 Commands

### `anbu checkpoint` (alias: `cp`)

Captures AI artifacts and attaches metadata trailers to the current Jujutsu commit.

```bash
anbu checkpoint [OPTIONS]
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--session <ID>` | `-s` | Capture a specific session by ID | — |
| `--latest` | | Auto-detect the most recent session across all agents | — |
| `--message <TEXT>` | `-m` | Description for this checkpoint | — |
| `--attach <FILE>...` | | Additional files to include manually | — |
| `--revision <REV>` | | Jujutsu revision to tag | `@` (working copy) |
| `--no-tag` | | Skip jj trailer attachment (store locally only) | `false` |

> **Note**: `--session` and `--latest` are mutually exclusive. If neither is provided and `--attach` is used, a manual checkpoint is created.

#### Examples

```bash
# Capture the latest AI session
anbu checkpoint --latest

# Capture a specific Antigravity session with a message
anbu checkpoint -s de6a2658-da26-4cb0-9d74-a4b6dc11aa93 -m "Auth middleware refactor"

# Attach extra files to the checkpoint
anbu checkpoint --latest --attach screenshots/demo.png --attach notes.md

# Store locally without tagging the jj commit
anbu checkpoint --latest --no-tag

# Tag a different revision
anbu checkpoint --latest --revision "mrsxzst"
```

#### Jujutsu Trailers

When a checkpoint is created (without `--no-tag`), ANBU appends trailers to the commit message via `jj describe`:

```
feat: implement OAuth flow

AI-Agent: antigravity
AI-Session: de6a2658-da26-4cb0-9d74-a4b6dc11aa93
AI-Checkpoint: c3d82ddd-faf2-4a00-a5a3-9eb98209a38d
```

These trailers are used by [Makimono](../makimono/) (the SHINOBI frontend) to display AI provenance badges and context panels alongside commits.

---

### `anbu log` (alias: `l`)

Lists checkpoints stored in the local SQLite index.

```bash
anbu log [OPTIONS]
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--limit <N>` | `-n` | Maximum number of checkpoints to display | `20` |

#### Example Output

```
 🥷 ANBU Log 4 checkpoint(s)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  c3d82ddd  2026-08-01 17:31  antigravity  Test Badge
           → commit 1a476c8 │ 📋 plan + ✅ task + 📝 walkthrough
  a1b2c3d4  2026-07-31 14:20  copilot      OAuth refactor
           → commit fa90b96 │ 💬 log + 📋 plan
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

### `anbu show` (alias: `s`)

Displays details of a specific checkpoint, or the content of an individual artifact.

```bash
anbu show <ID> [OPTIONS]
```

| Argument | Description |
|----------|-------------|
| `<ID>` | Checkpoint ID — full UUID or short prefix (e.g., `c3d82ddd`) |

| Option | Description |
|--------|-------------|
| `--artifact <NAME>` | Display the raw content of a specific artifact file |

#### Examples

```bash
# Show checkpoint summary
anbu show c3d82ddd

# Display the content of a specific artifact
anbu show c3d82ddd --artifact implementation_plan.md

# Pipe artifact content to another tool
anbu show c3d8 --artifact overview.txt | head -50
```

#### Example Output

```
  🥷 Checkpoint c3d82ddd-faf2-4a00-a5a3-9eb98209a38d
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Agent:    antigravity
  Session:  de6a2658-da26-4cb0-9d74-a4b6dc11aa93
  Commit:   1a476c80...
  Message:  Test Badge
  Created:  2026-08-01 17:31:09 UTC

  Artifacts:
    ✓ 📋 plan implementation_plan.md (6.4 KB)
    ✓ ✅ task task.md (910 B)
    ✓ 📝 walkthrough walkthrough.md (2.1 KB)
    ✓ 💬 log overview.txt (45.2 KB)
    ✓ 🖼️ media screenshot_1.webp (120.3 KB)

  Use anbu show c3d82ddd --artifact <name> to view an artifact.
```

---

### `anbu sessions`

Lists detected AI agent sessions available for capture.

```bash
anbu sessions [OPTIONS]
```

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--limit <N>` | `-n` | Maximum number of sessions to display | `10` |
| `--agent <TYPE>` | | Filter by agent: `antigravity`, `copilot` | all |
| `--all-workspaces` | | Show Copilot sessions from ALL VS Code workspaces | current only |

#### Session Sources

| Agent | Source Directory | Session ID |
|-------|-----------------|------------|
| Antigravity (Gemini) | `~/.gemini/antigravity/brain/<conversation-id>/` | Conversation UUID |
| GitHub Copilot | `~/.vscode/workspaceStorage/<hash>/chat/` | Chat session hash |

#### Examples

```bash
# List all sessions (both agents)
anbu sessions

# Only Antigravity sessions
anbu sessions --agent antigravity

# Only Copilot sessions from all workspaces
anbu sessions --agent copilot --all-workspaces

# Show more sessions
anbu sessions -n 30
```

---

### `anbu sync` (alias: `push`)

Synchronizes local checkpoints to the SHINOBI server (Taijutsu). After successful upload, local artifact files are purged (the SQLite index retains metadata).

```bash
anbu sync --owner <OWNER> --repo <REPO> [OPTIONS]
```

| Option | Description | Required |
|--------|-------------|----------|
| `--owner <OWNER>` | Repository owner handle | ✅ |
| `--repo <REPO>` | Repository name | ✅ |
| `--id <ID>` | Sync only a specific checkpoint (default: all unsynced) | ❌ |

#### Protocol

Checkpoints are sent as `multipart/form-data` (streaming) to:
```
POST /api/v1/repos/{owner}/{repo}/checkpoints
```

Authentication: Basic Auth with `login:PAT` from `~/.shinobi/config.toml`.

After the server responds `201 Created`, the local files in `.shinobi/anbu/checkpoints/{uuid}/` are deleted. The server stores artifacts on IPFS and metadata in PostgreSQL.

#### Examples

```bash
# Sync all unsynced checkpoints
anbu sync --owner YmClash --repo boruto

# Sync a specific checkpoint
anbu sync --owner YmClash --repo boruto --id c3d82ddd
```

---

### `anbu setup`

Configures the connection to your SHINOBI server. Launches an interactive wizard by default.

```bash
anbu setup [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--server-url <URL>` | Server URL (e.g., `http://localhost:3000`) |
| `--login <LOGIN>` | Username |
| `--pat <PAT>` | Personal Access Token |

#### Interactive Mode

```bash
anbu setup
# → Server URL: http://localhost:3000
# → Login: naruto
# → Personal Access Token (PAT): ****
# → Testing connection... ✓ connected
# ✅ Configuration saved to ~/.shinobi/config.toml
```

#### Silent Mode (CI/Scripting)

```bash
anbu setup --server-url http://shinobi.example.com --login naruto --pat shb_abc123
```

---

## ⚙️ Configuration

Configuration is stored in `~/.shinobi/config.toml` (created automatically on first run).

```toml
[anbu]
# Path to the Antigravity brain directory (default: ~/.gemini/antigravity/brain)
# antigravity_brain_path = "~/custom/path/brain"

# Path to the SQLite database (default: ~/.shinobi/anbu.db)
# database_path = "~/custom/anbu.db"

# Attach AI trailers to jj commits automatically (default: true)
auto_trailers = true

[server]
url = "http://localhost:3000"
login = "naruto"
pat = "shb_your_personal_access_token"
```

### File Locations

| File | Path | Purpose |
|------|------|---------|
| Configuration | `~/.shinobi/config.toml` | Global settings + server credentials |
| SQLite Index | `~/.shinobi/anbu.db` | Local checkpoint metadata (WAL mode) |
| Checkpoints | `.shinobi/anbu/checkpoints/{uuid}/` | Staged artifacts (per-repo, auto-purged after sync) |
| Manifests | `.shinobi/anbu/checkpoints/{uuid}/manifest.json` | Checkpoint metadata backup |
| Antigravity Brain | `~/.gemini/antigravity/brain/` | Source: Gemini conversation artifacts |
| Copilot Storage | `~/.vscode/workspaceStorage/` | Source: VS Code Copilot chat sessions |

---

## 🏗️ Architecture

```
anbu/src/
├── main.rs              # Entry point — command routing & handlers
├── cli.rs               # Clap CLI definitions (6 commands)
├── config.rs            # Configuration loading (~/.shinobi/config.toml)
├── models.rs            # Data models: Checkpoint, Artifact, AgentKind
├── setup.rs             # Interactive setup wizard (dialoguer)
├── sync.rs              # HTTP client: multipart upload to Taijutsu
├── collectors/
│   ├── mod.rs           # Collector trait + CollectedArtifact
│   ├── antigravity.rs   # Gemini/Antigravity brain scanner
│   └── copilot.rs       # GitHub Copilot workspaceStorage scanner
├── storage/
│   ├── mod.rs
│   ├── artifact_store.rs  # Filesystem storage (.shinobi/anbu/)
│   └── index.rs           # SQLite index (WAL mode, search, sync tracking)
└── vcs/
    ├── mod.rs
    └── jj_integration.rs  # Jujutsu trailer injection (jj describe)
```

### Data Flow

```
┌─────────────────┐     ┌───────────────┐     ┌──────────────┐
│  AI Agent        │     │  ANBU CLI     │     │  Taijutsu    │
│  (Antigravity/   │────▶│  checkpoint   │────▶│  Server      │
│   Copilot)       │scan │  + index      │sync │  (IPFS+PG)   │
└─────────────────┘     └───────┬───────┘     └──────┬───────┘
                                │                     │
                    jj describe │                     │ API
                                ▼                     ▼
                        ┌───────────────┐     ┌──────────────┐
                        │  Jujutsu      │     │  Makimono    │
                        │  Commit       │     │  (Next.js)   │
                        │  + Trailers   │     │  🧠 Badge    │
                        └───────────────┘     └──────────────┘
```

---

## 🧪 Tests

```bash
# Run all tests (26 tests)
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific module tests
cargo test --lib storage::index::tests
cargo test --lib vcs::jj_integration::tests
```

---

## 🔗 Integration with Makimono

ANBU checkpoints synced to Taijutsu are displayed in Makimono (the SHINOBI web frontend):

- **🧠 Badge** in the commit list — indicates AI-assisted commits
- **AI Context Tab** in commit detail — shows agent, session, IPFS CID, artifacts
- **IPFS View** — browse captured artifacts via the IPFS gateway proxy

The correlation between commits and checkpoints uses the `AI-Checkpoint: <uuid>` trailer in the commit message, which is resilient to rebases and history rewrites.

---

## 📋 Supported Agents

| Agent | Status | Session Detection | Artifacts Captured |
|-------|--------|-------------------|--------------------|
| Antigravity (Gemini) | ✅ Stable | `~/.gemini/antigravity/brain/` | Plans, tasks, walkthroughs, logs, media |
| GitHub Copilot | ✅ Stable | VS Code `workspaceStorage/` | Chat history, conversation logs |
| Cursor | 🔮 Planned | — | — |
| Claude Code | 🔮 Planned | — | — |

---

## 📄 License

MIT — Part of the SHINOBI project by [YmClash](https://github.com/YmClash).
