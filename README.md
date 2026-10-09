<p align="center">
  <br/>
  <strong>
    ███████╗██╗  ██╗██╗███╗   ██╗ ██████╗ ██████╗ ██╗<br/>
    ██╔════╝██║  ██║██║████╗  ██║██╔═══██╗██╔══██╗██║<br/>
    ███████╗███████║██║██╔██╗ ██║██║   ██║██████╔╝██║<br/>
    ╚════██║██╔══██║██║██║╚██╗██║██║   ██║██╔══██╗██║<br/>
    ███████║██║  ██║██║██║ ╚████║╚██████╔╝██████╔╝██║<br/>
    ╚══════╝╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝ ╚═════╝ ╚═════╝ ╚═╝<br/>
  </strong>
  <br/>
  <em>Next-Generation Federated Forge for Human/AI Collaboration</em>
  <br/><br/>
  <a href="#-quickstart"><img src="https://img.shields.io/badge/Rust-1.96+-orange?logo=rust" alt="Rust"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue" alt="License"></a>
  <a href="#-architecture"><img src="https://img.shields.io/badge/Architecture-Hexagonal-purple" alt="Architecture"></a>
  <a href="#-ai-triad"><img src="https://img.shields.io/badge/AI-Triad%20(3%20Agents)-green" alt="AI"></a>
  <a href="#-federation"><img src="https://img.shields.io/badge/Federation-ActivityPub%20%2F%20ForgeFed-pink" alt="Federation"></a>
  <a href="#%EF%B8%8F-native-cicd--jutsu-runner--kage-bunshin"><img src="https://img.shields.io/badge/CI%2FCD-Jutsu%20%2B%20Kage%20Bunshin-red" alt="CI/CD"></a>
  <a href="https://jjshinobi.dev"><img src="https://img.shields.io/badge/Live-jjshinobi.dev-black" alt="Live"></a>
</p>

---

## 🥷 What is Shinobi?

**Shinobi** is an experimental, next-generation **federated code forge** designed from the ground up for **human and AI collaboration**. It combines a Jujutsu-powered VCS, a complete project management suite (Merge Requests, Issues, Labels, Forks), a **native CI/CD runner with AI auto-healing**, a federated identity layer (ActivityPub/ForgeFed), and a triad of AI agents — all in a single, self-hostable platform.

### Key Capabilities

| Feature | Description |
|---------|-------------|
| 🧬 **Semantic Code Memory** | Every commit is chunked via Tree-sitter and vectorized (Nomic 256d) for similarity search |
| 🤖 **AI Triad** | Tensai (RAG archivist), Oracle (code reviewer), Sensei (chat mentor SSE + CI healer) |
| ⚡ **Jutsu Runner** | Native CI/CD — `jutsu.yml` at repo root, Docker stages (bollard), DAG `requires`, triggered on `git push` |
| ⚔️ **Kage Bunshin** | AI auto-healing CI/CD — Sensei diagnoses a failing stage, patches it, verifies in a shadow run, opens an MR |
| 🪝 **Chakra Webhooks** | Kafka→HTTP delivery, HMAC-SHA256, CloudEvents headers, SSRF guard, exponential retry |
| ⚔️ **Merge Requests** | Full lifecycle (create, review, approve, merge FF/Squash) + **cross-repo MR** from forks |
| 🍴 **Forks** | Local forks + **federated forks** (`Offer(Fork)` → `Accept(Offer)`) |
| 🎯 **Issues & Labels** | Unified `#ID` counter (shared MR/Issue), colored labels M:N, timeline, @mentions |
| 🔔 **Notifications** | In-app bell, MR/Review/Merge/Mention events, grouped by date |
| 🌍 **ActivityPub Federation** | WebFinger + NodeInfo + Inbox/Outbox + HTTP Signatures + federated `@user@domain` mentions |
| 🔀 **Git Smart HTTP** | `git`/`jj` push/pull/clone with PAT authentication and multi-tenant isolation |
| 🥷 **ANBU CLI** | AI checkpoint capture (Antigravity/Copilot → IPFS) + pipeline control from the terminal |
| 🎨 **4 Themes** | Ninja (dark), Cyberpunk (neon), Glass (blur), Scroll (parchment) |

---

## 🏗️ Architecture

Shinobi follows **Hexagonal Architecture** (Ports & Adapters) with strict dependency inversion across 5 Rust crates, a Next.js frontend and a standalone CLI.

> 📐 **Detailed Mermaid diagrams** are available in [`Docs/schema_V3/`](./Docs/schema_V3/) — global architecture, hexagonal layers, Docker infrastructure, database ERD, E2E pipeline, federation flows and cross-repo MR lifecycle.

```
┌───────────────────────────────────────────────────────────────────────┐
│               📜 MAKIMONO — Frontend [Next.js 16 + Bun]              │
│  35 routes · 4 themes · SWR polling · Jutsus ⚡ dashboard · HealPanel │
├───────────────────────────────────────────────────────────────────────┤
│               🧠 TAIJUTSU — Core Engine [Rust + Axum + Tokio]        │
│                                                                       │
│  🌐 REST (84+ routes)  ⚡ gRPC (7 RPCs)  🔀 Git HTTP (3 endpoints)  │
│  🔐 Auth (JWT + PAT + GitHub OAuth + RBAC · 3 layers)                │
│  ⚙️  50+ Use Cases · Service Accounts (bots IA first-class)          │
│  🔄 7 Workers: Purge · Inbox · Chakra ×2 · Jutsu · Kage Bunshin · …  │
├────────────┬──────────┬─────────────┬──────────┬─────────────────────┤
│ 💾 Fūinjutsu│ 🐝 Genjutsu│ 📨 Nen      │ 📁 VCS    │ 🤖 Triade IA       │
│ PostgreSQL │ IPFS Kubo│ Kafka KRaft │ jj-lib   │ Tensai+Oracle+Sensei│
│ + pgvector │ Merkle   │ VCS · Chakra│ 100%     │ 2× Ollama + Nomic  │
│ + Redis    │ DAG IPLD │ Jutsu · KB  │ natif    │ ONNX 256d          │
├────────────┴──────────┴─────────────┴──────────┴─────────────────────┤
│        ⚡ JUTSU RUNNER + ⚔️ KAGE BUNSHIN — Native CI/CD (bollard)     │
│  jutsu.yml · Docker stages · log sanitization · shadow re-run · MR   │
├───────────────────────────────────────────────────────────────────────┤
│            🌍 FÉDÉRATION — ActivityPub / ForgeFed                     │
│  WebFinger · NodeInfo 2.1 · RSA-2048 Signatures · Signed Fetch       │
│  FanoutService (Semaphore 10) · InboxWorker (FIFO, Poison Pill safe) │
├───────────────────────────────────────────────────────────────────────┤
│            👁️ DŌJUTSU — Observabilité                                │
│  Prometheus (scrape 15s) · Grafana (17 panels, 4 sections)           │
└───────────────────────────────────────────────────────────────────────┘
```

| Subsystem | Name | Technology | Purpose |
|-----------|------|------------|---------|
| Core Engine | **Taijutsu** | Rust, Axum, Tonic, Tokio | Backend — REST + gRPC + Git HTTP, auth, DI |
| VCS Engine | — | jj-lib 0.41 (GitBackend) | Multi-tenant atomic commits, diff, fork remotes — zero `git` CLI |
| AI & Semantics | **Tensai** | Tree-sitter, Nomic, pgvector | Chunking (5 langs), embedding (256d), RAG search |
| AI Review | **Oracle** | Ollama, Granite3 2B | Automated code review with scoring |
| AI Chat & Healing | **Sensei** | Ollama, SmolLM2 1.7B | SSE chat mentor + Kage Bunshin diagnosis |
| CI/CD | **Jutsu Runner** | bollard (Docker API), Kafka | `jutsu.yml` pipelines, commit statuses |
| Auto-Healing | **Kage Bunshin** | Sensei + shadow workspace | Patch hunks → shadow re-run → auto MR |
| Webhooks | **Chakra** | Kafka, Redis ZSET, HMAC | Outgoing event delivery with retry |
| Database | **Fūinjutsu** | PostgreSQL 17, pgvector, Redis 8 | 30 migrations, HNSW vector index, caching |
| Storage | **Genjutsu** | IPFS Kubo | Content-addressable Merkle DAG + AI artifacts |
| Events | **Nen** | Apache Kafka (KRaft) | Async event propagation (VCS, webhooks, pipelines, healing) |
| Federation | — | ActivityPub, ForgeFed | WebFinger, Inbox/Outbox, HTTP Signatures, federated forks |
| CLI | **ANBU** | Rust standalone binary | AI checkpoint capture & sync + `anbu jutsu` |
| Frontend | **Makimono** | Next.js 16, Bun 1.3.8 | 35 routes, 4 themes, SWR, Shiki |
| Monitoring | **Dōjutsu** | Prometheus + Grafana | 17 panels, 4 dashboard sections |

### Workspace Structure

```
shinobi/
├── README.md
├── LICENSE                             # MIT
├── CODEX_BOARD.md                      # Architecture journal Vol. I   (Phases 1 → 12A)
├── CODEX_BOARD_1.md                    # Architecture journal Vol. II  (Phases 6B → 38B)
├── CODEX_BOARD_2.md                    # Architecture journal Vol. III (Phases 37E → 41-C)
├── docker-compose.prod.yml             # Prod (11 services, Cloudflare Tunnel)
├── Docs/
│   ├── jutsu-runner.md                 # Native CI/CD reference
│   ├── kage-bunshin.md                 # Auto-healing reference
│   ├── kage-bunshin-walkthrough.md     # Implementation walkthrough (Phase 41)
│   ├── ci-cd-integration.md            # External CI (Drone / Woodpecker / Jenkins)
│   ├── test-e2e-scenario.md            # End-to-end user test scenario
│   ├── FORGE_COMPARISON.md             # Shinobi vs other forges
│   ├── examples/jutsu.yml              # Example pipeline
│   └── schema_V1/ · schema_V2/ · schema_V3/   # Mermaid diagrams
│
├── anbu/                               # ANBU CLI (standalone Rust crate)
├── dojutsu/                            # Prometheus + Grafana provisioning
│
├── makimono/                           # Frontend (Next.js 16 + Bun)
│   └── src/
│       ├── app/                        # 35 routes (App Router)
│       ├── components/                 # explorer, issue, MR, pipeline (StageRow, LogDrawer, HealPanel)…
│       ├── hooks/                      # SWR hooks (use-api, use-mr, use-issues, use-pipelines…)
│       ├── lib/                        # API clients, auth, cache, shiki
│       └── styles/                     # CSS modules (issues, federation, mr, pipeline…)
│
└── taijutsu/                           # Cargo Workspace (5 crates + binary)
    ├── Cargo.toml                      # Workspace root
    ├── docker-compose.yml              # Dev (PG + Redis + Kafka + IPFS + Ollama)
    ├── .env / .env.example             # Configuration
    ├── proto/shinobi.proto             # gRPC schema (7 RPCs)
    ├── migrations/                     # 30 SQL migrations (001 → 030)
    ├── src/
    │   ├── main.rs                     # Bootstrap + DI + Workers
    │   ├── config.rs                   # Env vars loader
    │   ├── jutsu_consumer.rs           # Pipeline Kafka consumer
    │   └── kage_bunshin_consumer.rs    # Auto-healing Kafka consumer
    └── crates/
        ├── domain/                     # Pure domain (0 tech deps) — entities, ports, errors
        ├── application/                # 50+ use cases (orchestration)
        ├── infrastructure/             # Adapters (PG, Kafka, IPFS, jj, Ollama, Docker, Federation)
        ├── presentation/               # REST + gRPC + Git HTTP + SharedState
        └── tensai/                     # Semantic chunker (Tree-sitter AST, 5 langs)
```

---

## 🌐 Live Instance

| Service | Public URL | Local dev |
|---------|-----------|-----------|
| **Makimono** (UI) | [`https://jjshinobi.dev`](https://jjshinobi.dev) | `http://localhost:3001` |
| **Taijutsu** (API + Git) | `https://api.jjshinobi.dev` | `http://localhost:3000` |
| **Git clone** | `https://api.jjshinobi.dev/{owner}/{repo}.git` | `http://localhost:3000/{owner}/{repo}.git` |

> Exposed through a **Cloudflare Tunnel**. The clone URL shown in the UI is configurable with `NEXT_PUBLIC_GIT_URL`.

---

## 🚀 Quickstart

### Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| **Rust** | 1.96+ | [rustup.rs](https://rustup.rs/) |
| **Docker** | 24+ | [docker.com](https://www.docker.com/) |
| **Bun** | 1.3+ | [bun.sh](https://bun.sh/) |
| **Protoc** | 35+ | `winget install Google.Protobuf` |
| **CMake** | 3.28+ | `winget install Kitware.CMake` |
| **Jujutsu** *(optional)* | latest | [jj-vcs.github.io](https://jj-vcs.github.io/jj/) |

### 1. Clone & Setup

```bash
git clone https://github.com/YmClash/shinobi.git
cd shinobi/taijutsu
cp .env.example .env
```

### 2. Start Infrastructure

```bash
docker compose up -d
docker compose ps   # Verify all services are healthy
```

This starts **PostgreSQL** (pgvector, :5432), **Redis** (:6379), **Kafka** (KRaft, :9092), **IPFS Kubo** (:5001) and the **Ollama** instances.

> The Jutsu Runner talks to the **local Docker daemon** to execute pipeline stages — Docker must stay running.

### 3. Run Migrations

```powershell
# PowerShell (Windows)
Get-ChildItem migrations\*.sql | Sort-Object Name | ForEach-Object {
    Get-Content $_.FullName -Raw | docker exec -i shinobi-postgres psql -U shinobi -d shinobi_dev
}
```

### 4. Run Backend

```bash
cargo run
```

First launch downloads the Nomic-Embed-Text-v1.5 model (~522 MB, cached in `.fastembed_cache/`).

### 5. Run Frontend

```bash
cd ../makimono
bun install
bun run dev    # http://localhost:3001
```

### 6. Verify

```bash
curl http://localhost:3000/health
curl http://localhost:3000/api/v1/status
```

### 7. Install ANBU (optional)

```bash
cd ../anbu
cargo install --path .
anbu setup     # interactive server + PAT configuration
```

> 🧪 Want to try everything as a real user? Follow [`Docs/test-e2e-scenario.md`](./Docs/test-e2e-scenario.md) (6 acts, 35 checkpoints).

---

## ⚡️ Native CI/CD — Jutsu Runner + Kage Bunshin

Drop a `jutsu.yml` at the root of any repository — every `git push` triggers a pipeline executed in Docker containers on the Shinobi host.

```yaml
name: "Rust Pipeline with Auto-Heal"
on: [push]                 # push | mr_created | tag

stages:
  Build:
    image: "rust:1.96-slim"
    kage_bunshin: true     # ⚔️ let Sensei try to fix this stage if it fails
    jutsus:
      - "cargo build --release"

  Test:
    image: "rust:1.96-slim"
    requires: [Build]      # DAG dependencies (cycle-checked)
    kage_bunshin: true
    jutsus:
      - "cargo test --workspace"
```

### ⚔️ Kage Bunshin (影分身) — the auto-healing flow

```
Stage ❌ failure (kage_bunshin: true)
  → stage = Healing, pipeline stays "running"
  → Kafka: shinobi.jutsu.kage-bunshin → KageBunshinConsumer
  → Sensei reads logs + targeted files → {diagnosis, hunks[], confidence}
  → confidence ≥ 0.5 → apply hunks in an ephemeral shadow workspace
  → shadow Docker re-run
      ✅ exit 0  → Healed → auto MR by shinobi-sensei-bot
      ❌ exit ≠0 → Failed (original code untouched)
  → Makimono: ⚔️ badge + HealPanel (diagnosis, colored diff, confidence, MR link)
```

### Run pipelines from the terminal (ANBU)

```bash
anbu jutsu trigger --ref main        # remote trigger on the Shinobi server
anbu jutsu logs --last -f            # follow logs live (zero-flicker TUI)
anbu jutsu run --local               # run jutsu.yml locally via your Docker
anbu jutsu run --local --dry-run     # validate the YAML only
```

> 📚 Full references: [`Docs/jutsu-runner.md`](./Docs/jutsu-runner.md) · [`Docs/kage-bunshin.md`](./Docs/kage-bunshin.md) · [`Docs/ci-cd-integration.md`](./Docs/ci-cd-integration.md) (external CI via Commit Status API)

---

## 📡 API Overview

### REST API — 84+ Routes (Port 3000)

| Category | Routes | Auth |
|----------|--------|------|
| **System** | `GET /health`, `/api/v1/status`, `/metrics` | Public |
| **Auth** | register, login, GitHub OAuth, PAT CRUD | Mixed |
| **Repos** | CRUD, visibility, soft delete, clone URL, fork | Semi-Public / Private |
| **Git HTTP** | info/refs, receive-pack, upload-pack | PAT (Basic Auth) |
| **Operations** | create, list, get, diff, diff-content | Semi-Public / Private |
| **Chunks** | search by name, semantic RAG search | Semi-Public |
| **Merge Requests** | create (incl. cross-repo), list, get, review, merge, close, diff | Semi-Public / Private |
| **Issues** | create, list, get, update, close, reopen, comment | Semi-Public / Private |
| **Labels** | create, list, delete, assign, unassign | Semi-Public / Private |
| **Pipelines** | list, detail, stages, trigger, **heals** | Semi-Public / Private |
| **Commit Statuses** | report / list external CI statuses | PAT |
| **Webhooks** | CRUD, deliveries, ping, regenerate secret | Private (owner) |
| **Notifications** | list, unread count, mark read | Private |
| **Sensei** | chat (SSE), models, warmup | Private |
| **ANBU** | checkpoint upload, list, IPFS proxy | Private |
| **Federation** | WebFinger, NodeInfo, Actor, Inbox, Outbox, Followers | Mixed |

### gRPC — Ninpo Protocol (Port 50051)

| RPC | Description |
|-----|-------------|
| `Ping` | Availability check |
| `CreateOperation` | Create VCS operation |
| `GetOperation` / `ListOperations` | Query operations |
| `GetChunksByOperation` | Semantic chunks |
| `SearchChunksByName` | Symbol search |
| `SemanticSearch` | RAG vector search |

---

## 🤖 AI Triad

Shinobi employs three specialized AI agents working in concert:

```
┌──────────────┐   ┌──────────────┐   ┌──────────────┐
│   TENSAI     │   │   ORACLE     │   │   SENSEI     │
│  Archiviste  │   │  Reviewer    │   │ Mentor+Healer│
│ Nomic + RAG  │   │ Granite3 2B  │   │ SmolLM2 1.7B │
│  (pgvector)  │   │   Ollama     │   │   Ollama     │
│  Async Kafka │   │  Async Kafka │   │ SSE + Kafka  │
└──────────────┘   └──────────────┘   └──────────────┘
```

| Agent | Model | Role | Interface |
|-------|-------|------|-----------|
| **Tensai** | Nomic-Embed-Text-v1.5 (ONNX, 256d) | Semantic chunking + vectorization + RAG | Kafka consumer |
| **Oracle** | Granite3 Dense 2B (Ollama) | Automated code review, score 0–100 | Kafka consumer |
| **Sensei** | SmolLM2 1.7B (Ollama) | Chat mentor with RAG context + Kage Bunshin CI diagnosis | SSE streaming + Kafka consumer |

AI work is traceable: **ANBU** checkpoints are stored in IPFS and linked to commits via an `ai-checkpoint:{CID}` trailer, and Sensei's fixes are authored by a dedicated **service account** (`shinobi-sensei-bot`).

### Semantic Search (RAG)

```bash
curl -X POST http://localhost:3000/api/v1/chunks/semantic-search \
  -H "Content-Type: application/json" \
  -d '{"query": "user authentication handler", "limit": 5, "threshold": 0.3}'
```

---

## 🌍 Federation

Shinobi implements **ActivityPub** and **ForgeFed** for decentralized code forge interoperability:

- **WebFinger** — `/.well-known/webfinger?resource=acct:handle@domain`
- **NodeInfo 2.1** — Software identification (`shinobi`)
- **Actor Profiles** — RSA-2048 public keys for HTTP Signature verification
- **Signed Fetch** — compatible with Mastodon `AUTHORIZED_FETCH`
- **Inbox** — Receives Follow, Create, Push, Update, Delete, Announce, Undo, Offer activities
- **Outbox** — Auto-publishes Create/Push activities on git push
- **Federated Forks** — `Offer(Fork)` → `Accept(Offer)` with aggregated fork count
- **Federated Mentions** — `@handle@domain` → WebFinger → signed `Create { Note { tag: Mention } }`
- **FanoutService** — Signed delivery to followers (Semaphore 10, non-blocking)
- **InboxWorker** — Background tokio loop (poll 30s, batch 20, Poison Pill safe, FIFO)

> 📐 See [`Docs/schema_V3/federation-activitypub.mmd`](./Docs/schema_V3/federation-activitypub.mmd) for the complete sequence diagram.

---

## 🧪 Testing

```bash
# Taijutsu
cd taijutsu
cargo test --workspace                 # Unit tests
cargo test --workspace -- --ignored    # Integration tests (Docker required)

# ANBU
cd ../anbu
cargo test

# Makimono
cd ../makimono
npx tsc --noEmit
```

**Current status:** 272+ tests passing, 0 failed (Taijutsu 261+ · ANBU 37) · TypeScript 0 errors.

---

## ⚙️ Configuration

All configuration via environment variables (loaded from `.env`). Key variables:

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string |
| `REST_PORT` | Axum HTTP server port (default: 3000) |
| `GRPC_PORT` | Tonic gRPC port (default: 50051) |
| `VCS_WORKSPACE_ROOT` | jj-lib multi-tenant storage path |
| `JWT_SECRET` | Secret for JWT token signing |
| `GITHUB_CLIENT_ID` / `SECRET` | GitHub OAuth credentials |
| `KAFKA_BROKERS` | Kafka brokers (default: localhost:9092) |
| `IPFS_API_URL` | IPFS Kubo RPC endpoint |
| `ORACLE_OLLAMA_URL` | Oracle Ollama instance URL |
| `SENSEI_OLLAMA_URL` | Sensei Ollama instance URL |
| `FEDERATION_DOMAIN` | Public domain for ActivityPub (e.g. `api.jjshinobi.dev`) |
| `FEDERATION_ENABLED` | Enable/disable federation features |
| `EMBEDDING_ENABLED` | Enable/disable RAG embeddings |
| `CHAKRA_ENABLED` / `CHAKRA_TOPIC` | Webhooks pipeline (default topic: `shinobi.events.webhooks`) |
| `JUTSU_TOPIC` | Pipeline events topic (default: `shinobi.jutsu.pipeline`) |
| `KAGE_BUNSHIN_ENABLED` | Enable AI auto-healing globally (default: `true`) |
| `KAGE_BUNSHIN_TOPIC` | Healing topic (default: `shinobi.jutsu.kage-bunshin`) |
| `KAGE_BUNSHIN_CONFIDENCE_THRESHOLD` | Minimum Sensei confidence to apply a patch (default: `0.5`) |
| `KAGE_BUNSHIN_SHADOW_TIMEOUT_SECS` | Shadow re-run timeout (default: `120`) |
| `NEXT_PUBLIC_GIT_URL` *(Makimono)* | Base URL shown for clone commands (default: `https://api.jjshinobi.dev`) |

> See `taijutsu/.env.example` for the complete list.

---

## 📦 Tech Stack

| Category | Technology |
|----------|------------|
| Language | Rust 1.96 (Edition 2024) |
| Async Runtime | Tokio |
| REST | Axum 0.8 |
| gRPC | Tonic 0.14 + Protobuf |
| Database | PostgreSQL 17 + pgvector + Redis 8 |
| VCS Engine | jj-lib 0.41.0 (GitBackend, multi-tenant, native `jj_lib::git`) |
| Event Bus | Apache Kafka (KRaft, rdkafka) |
| Distributed Storage | IPFS Kubo (Merkle DAG IPLD) |
| CI/CD Execution | bollard (Docker Engine API) |
| AI Embeddings | fastembed (Nomic-Embed-Text-v1.5, ONNX 256d) |
| AI LLMs | Ollama (Granite3 2B + SmolLM2 1.7B) |
| Semantic Parsing | Tree-sitter 0.25 (Rust, TS, TSX, CSS, Python) |
| Frontend | Next.js 16 (Turbopack) + Bun 1.3.8 |
| Code Highlighting | Shiki 4.2 |
| Auth | JWT + PAT + GitHub OAuth + RBAC + Service Accounts |
| Federation | ActivityPub + ForgeFed + HTTP Signatures (Draft-Cavage-12) |
| Webhooks | HMAC-SHA256 + CloudEvents 1.0 |
| Observability | tracing + Prometheus + Grafana (17 panels) |
| Exposure | Cloudflare Tunnel (`jjshinobi.dev`) |
| Error Handling | thiserror (domain) + anyhow (binary) |

---

## 🔑 Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Hexagonal Architecture** | Domain stays pure — no framework deps. Adapters are swappable. |
| **jj-lib (not Git directly)** | First-class conflicts, anonymous branching, operation log — zero `git` subprocess in the VCS engine |
| **Unified ticket counter** | Issues and MRs share `next_ticket_number` — `#ID` is unambiguous per repo |
| **Native CI/CD** | `jutsu.yml` + Docker via bollard — no external runner required (external CI still supported via Commit Status API) |
| **Dedicated healing topic** | Kage Bunshin runs on its own Kafka topic/consumer so slow LLM calls never block regular pipelines |
| **Shadow workspace verification** | An AI patch is only proposed (as an MR) after it passes a real re-run — never pushed directly |
| **Bots as first-class actors** | Sensei's fixes are authored by a service account — full provenance & RBAC |
| **Nomic over MiniLM** | MTEB ~59.4 vs ~56.3, 8192 token context (16×), Matryoshka support |
| **ONNX local (not API)** | Zero network latency, zero cost, offline-capable |
| **pgvector HNSW** | No VACUUM required, consistent quality, O(log n) |
| **Dual Ollama instances** | Oracle (review) and Sensei (chat/heal) run separately — zero contention |
| **3-layer auth** | Public / Semi-Public / Private — Bouclier Global enforced at router level |
| **ActivityPub federation** | Decentralized, no vendor lock-in, interop with Mastodon/Forgejo |
| **Fire-and-forget events** | Webhooks/notifications never block the business operation (graceful degradation) |
| **Poison Pill protection** | Inbox Worker always marks activities as processed, even on error — prevents infinite loops |
| **Manual mocks (no framework)** | Zero compile-time overhead, total control, zero macro magic |

---

## 🛣️ Roadmap

- [x] **Phases 1–9** — Foundations, hexagonal arch, VCS, IPFS, Kafka, AI Tensai, Oracle, RAG vectoriel
- [x] **Phases 10–12** — Multi-tenant forge, Git Smart HTTP, sync hooks
- [x] **Phases 14–18** — Oracle UI, Sensei chat, commits/diff, SWR cache
- [x] **Phases 19–21** — Auth (JWT + PAT + OAuth), GitHub import, VCS multi-tenant
- [x] **Phases 22–25** — Kafka decouple, private repos, soft delete, service accounts, profiles
- [x] **Phase 26** — Merge Requests (full lifecycle + Optimistic UI)
- [x] **Phases 27–27quater** — ActivityPub/ForgeFed federation (WebFinger → Inbox → Outbox → Fanout)
- [x] **Phases 28–28F** — ANBU CLI (AI checkpoint capture, Copilot collector, IPFS artifacts)
- [x] **Phases 29–32** — AI Provenance gutter, UnifiedDiffViewer V2, Federation Dashboard, Inbox Worker
- [x] **Phase 33** — Issues/Tickets (Le Parchemin des Doléances)
- [x] **Phase 34 (V1→V4)** — Chakra Webhooks (Kafka→HTTP, auto-emission, ANBU audit, dashboard)
- [x] **Phases 35–36** — Tech-debt liquidation + Sync Hook surgery
- [x] **Phase 37 (A→F)** — Public profile, forks, @mentions, federated forks, cross-repo MR, federated mentions
- [x] **Phase 38** — Notifications (in-app bell + redesign)
- [x] **Phase 39** — External CI bridge (Commit Status API + Drone/Woodpecker/Jenkins guides)
- [x] **Phase 40 / 40-C / 40-E** — Jutsu Runner native CI/CD (backend + Makimono dashboard + `anbu jutsu`)
- [x] **Phase 41 / 41-B / 41-C** — Kage Bunshin auto-healing (backend + HealPanel UI + docs & `jjshinobi.dev`)
- [ ] **E2E validation** — Run [`Docs/test-e2e-scenario.md`](./Docs/test-e2e-scenario.md) on a fresh repository
- [ ] **Git over `jjshinobi.dev`** — Proxy Git Smart HTTP through the UI domain (or `git.jjshinobi.dev`)
- [ ] **Phase 42** — Swarm Mode 🐝 (Architect Tensai + Auditor Oracle on MRs, optional Ronin agent)
- [ ] **Phase 37F-Fix3** — Mastodon silent drop on federated mention notifications
- [ ] **Phase 28G** — Service Worker IPFS (decentralized browser resolution)
- [ ] **Production hardening** — TLS reverse proxy, rate limiting

> 📜 Full architectural journal: [`CODEX_BOARD.md`](./CODEX_BOARD.md) (Vol. I) · [`CODEX_BOARD_1.md`](./CODEX_BOARD_1.md) (Vol. II) · [`CODEX_BOARD_2.md`](./CODEX_BOARD_2.md) (Vol. III)

---

## 📄 License

[MIT](LICENSE) — Copyright © 2026 YmClash
