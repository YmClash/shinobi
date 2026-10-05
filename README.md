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
</p>

---

## 🥷 What is Shinobi?

**Shinobi** is an experimental, next-generation **federated code forge** designed from the ground up for **human and AI collaboration**. It combines a Jujutsu-powered VCS, a complete project management suite (Merge Requests, Issues, Labels), a federated identity layer (ActivityPub/ForgeFed), and a triad of AI agents — all in a single, self-hostable platform.

### Key Capabilities

| Feature | Description |
|---------|-------------|
| 🧬 **Semantic Code Memory** | Every commit is chunked via Tree-sitter and vectorized (Nomic 256d) for similarity search |
| 🤖 **AI Triad** | Tensai (archiviste RAG), Oracle (code reviewer), Sensei (chat mentor SSE) |
| 🌍 **ActivityPub Federation** | WebFinger + NodeInfo + Inbox/Outbox + HTTP Signatures — interop with the Fediverse |
| ⚔️ **Merge Requests** | Full lifecycle: create, review, approve, merge (FF/Squash), diff viewer (Unified/Split) |
| 🎯 **Issues & Labels** | Tickets with unified `#ID` counter (shared MR/Issue), colored labels M:N, timeline events |
| 🔀 **Git Smart HTTP** | `git push/pull/clone` with PAT authentication and multi-tenant isolation |
| 🥷 **ANBU CLI** | AI checkpoint capture — exfiltrates Antigravity/Copilot sessions to IPFS |
| 🎨 **4 Themes** | Ninja (dark), Cyberpunk (neon), Glass (blur), Scroll (parchment) |

---

## 🏗️ Architecture

Shinobi follows **Hexagonal Architecture** (Ports & Adapters) with strict dependency inversion across 5 Rust crates + a Next.js frontend.

> 📐 **Detailed Mermaid diagrams** are available in [`Docs/schema_V2/`](./Docs/schema_V2/) — covering global architecture, hexagonal layers, Docker infrastructure, database ERD, E2E pipeline, and federation flows.

```
┌───────────────────────────────────────────────────────────────────────┐
│               📜 MAKIMONO — Frontend [Next.js 16 + Bun]              │
│  31 routes · 4 themes · SWR cache · Sensei chat · Diff viewer        │
├───────────────────────────────────────────────────────────────────────┤
│               🧠 TAIJUTSU — Core Engine [Rust + Axum + Tokio]        │
│                                                                       │
│  🌐 REST (68 routes)  ⚡ gRPC (7 RPCs)  🔀 Git HTTP (3 endpoints)   │
│  🔐 Auth (JWT + PAT + GitHub OAuth + RBAC · 3 layers)                │
│  ⚙️  44 Use Cases · 💎 12 Domain Ports · ❌ 12 Error variants        │
│  🔄 Workers: Purge Timer + Inbox Worker                               │
├────────────┬──────────┬─────────────┬──────────┬─────────────────────┤
│ 💾 Fūinjutsu│ 🐝 Genjutsu│ 📨 Nen      │ 📁 VCS    │ 🤖 Triade IA       │
│ PostgreSQL │ IPFS Kubo│ Kafka KRaft │ jj-lib   │ Tensai+Oracle+Sensei│
│ + pgvector │ Merkle   │ 2 topics    │ GitBack  │ 2× Ollama + Nomic  │
│ + Redis    │ DAG IPLD │             │ Multi-T  │ ONNX 256d          │
├────────────┴──────────┴─────────────┴──────────┴─────────────────────┤
│            🌍 FÉDÉRATION — ActivityPub / ForgeFed                     │
│  WebFinger · NodeInfo 2.1 · RSA-2048 Signatures · Inbox/Outbox       │
│  FanoutService (Semaphore 10) · InboxWorker (FIFO, Poison Pill safe) │
├───────────────────────────────────────────────────────────────────────┤
│            👁️ DŌJUTSU — Observabilité                                │
│  Prometheus (scrape 15s) · Grafana (17 panels, 4 sections)           │
└───────────────────────────────────────────────────────────────────────┘
```

| Subsystem | Name | Technology | Purpose |
|-----------|------|------------|---------|
| Core Engine | **Taijutsu** | Rust, Axum, Tonic, Tokio | Backend — REST + gRPC + Git HTTP, auth, DI |
| VCS Engine | — | jj-lib 0.41 (GitBackend) | Multi-tenant atomic commits, tree building, diff |
| AI & Semantics | **Tensai** | Tree-sitter, Nomic, pgvector | Chunking (5 langs), embedding (256d), RAG search |
| AI Review | **Oracle** | Ollama, Granite3 2B | Automated code review with scoring |
| AI Chat | **Sensei** | Ollama, SmolLM2 1.7B | Real-time SSE chat mentor |
| Database | **Fūinjutsu** | PostgreSQL 17, pgvector, Redis 8 | 19 migrations, HNSW vector index, caching |
| Storage | **Genjutsu** | IPFS Kubo | Content-addressable Merkle DAG + AI artifacts |
| Events | **Nen** | Apache Kafka (KRaft) | Async event propagation (2 topics) |
| Federation | — | ActivityPub, ForgeFed | WebFinger, Inbox/Outbox, HTTP Signatures |
| CLI | **ANBU** | Rust standalone binary | AI checkpoint capture & sync |
| Frontend | **Makimono** | Next.js 16, Bun 1.3.8 | 31 routes, 4 themes, SWR, Shiki |
| Monitoring | **Dōjutsu** | Prometheus + Grafana | 17 panels, 4 dashboard sections |

### Workspace Structure

```
shinobi/
├── README.md
├── LICENSE                             # MIT
├── CODEX_BOARD.md                      # Architecture journal Vol. I
├── CODEX_BOARD_1.md                    # Architecture journal Vol. II (Phases 6B→33)
├── Docs/
│   ├── schema_V1/                      # Original Mermaid diagrams (Phase 8 era)
│   └── schema_V2/                      # Current diagrams (Post-Phase 33)
│       ├── architecture-globale.mmd
│       ├── architecture-hexagonale.mmd
│       ├── infrastructure-docker.mmd
│       ├── database-schema.mmd
│       ├── pipeline-e2e.mmd
│       └── federation-activitypub.mmd
│
├── makimono/                           # Frontend (Next.js 16 + Bun)
│   └── src/
│       ├── app/                        # 31 routes (App Router)
│       ├── components/                 # UI components (explorer, issue, MR, etc.)
│       ├── hooks/                      # SWR hooks (use-api, use-mr, use-issues, etc.)
│       ├── lib/                        # API clients, auth, cache, shiki
│       └── styles/                     # CSS modules (issues.css, federation.css, mr.css)
│
└── taijutsu/                           # Cargo Workspace (5 crates + binary)
    ├── Cargo.toml                      # Workspace root
    ├── docker-compose.yml              # Dev (PG + Redis + Kafka + IPFS)
    ├── docker-compose.prod.yml         # Prod (11 services Zero Trust)
    ├── .env / .env.example             # Configuration (~55 vars)
    ├── proto/shinobi.proto             # gRPC schema (7 RPCs)
    ├── migrations/                     # 19 SQL migrations
    ├── src/
    │   ├── main.rs                     # Bootstrap + DI + Workers
    │   └── config.rs                   # Env vars loader
    └── crates/
        ├── domain/                     # Pure domain (0 tech deps)
        │   └── entities, ports, errors
        ├── application/                # 44 use cases (orchestration)
        ├── infrastructure/             # Adapters (PG, Kafka, IPFS, jj, Ollama, Federation)
        ├── presentation/               # REST + gRPC + Git HTTP + SharedState
        └── tensai/                     # Semantic chunker (Tree-sitter AST, 5 langs)
```

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

This starts **PostgreSQL** (pgvector, :5432), **Redis** (:6379), **Kafka** (KRaft, :9092), and **IPFS Kubo** (:5001).

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

---

## 📡 API Overview

### REST API — 68 Routes (Port 3000)

| Category | Routes | Auth |
|----------|--------|------|
| **System** | `GET /health`, `/api/v1/status`, `/metrics` | Public |
| **Auth** | register, login, GitHub OAuth, PAT CRUD | Mixed |
| **Repos** | CRUD, visibility, delete, clone URL | Semi-Public / Private |
| **Git HTTP** | info/refs, receive-pack, upload-pack | PAT (Basic Auth) |
| **Operations** | create, list, get, diff, diff-content | Semi-Public / Private |
| **Chunks** | search by name, semantic RAG search | Semi-Public |
| **Merge Requests** | create, list, get, review, merge, close, diff | Semi-Public / Private |
| **Issues** | create, list, get, update, close, reopen, comment | Semi-Public / Private |
| **Labels** | create, list, delete, assign, unassign | Semi-Public / Private |
| **Sensei** | chat (SSE), models, warmup | Private |
| **ANBU** | checkpoint upload, list | Private |
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
│  Archiviste  │   │  Reviewer    │   │   Mentor     │
│ Nomic + RAG  │   │ Granite3 2B  │   │ SmolLM2 1.7B │
│  (pgvector)  │   │ Ollama :11435│   │ Ollama :11436│
│  Async Kafka │   │  Async Kafka │   │  Sync SSE    │
└──────────────┘   └──────────────┘   └──────────────┘
```

| Agent | Model | Role | Interface |
|-------|-------|------|-----------|
| **Tensai** | Nomic-Embed-Text-v1.5 (ONNX, 256d) | Semantic chunking + vectorization + RAG | Kafka consumer |
| **Oracle** | Granite3 Dense 2B (Ollama) | Automated code review, score 0–100 | Kafka consumer |
| **Sensei** | SmolLM2 1.7B (Ollama) | Real-time chat mentor with RAG context | SSE streaming |

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
- **Inbox** — Receives Follow, Create, Push, Update, Delete, Announce, Undo activities
- **Outbox** — Auto-publishes Create/Push activities on git push
- **FanoutService** — Signed delivery to followers (Semaphore 10, non-blocking)
- **InboxWorker** — Background tokio loop (poll 30s, batch 20, Poison Pill safe, FIFO)

> 📐 See [`Docs/schema_V2/federation-activitypub.mmd`](./Docs/schema_V2/federation-activitypub.mmd) for the complete sequence diagram.

---

## 🧪 Testing

```bash
cargo test --workspace          # Unit tests (198+ passing)
cargo test --workspace -- --ignored   # Integration tests (Docker required)
```

**Current coverage:** 198+ tests (domain 51 + application 66 + infra 58 + presentation 12 + tensai 11)

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
| `FEDERATION_DOMAIN` | Public domain for ActivityPub (e.g. forge.example.com) |
| `FEDERATION_ENABLED` | Enable/disable federation features |
| `EMBEDDING_ENABLED` | Enable/disable RAG embeddings |

> See `.env.example` for the complete list (~55 variables).

---

## 📦 Tech Stack

| Category | Technology |
|----------|------------|
| Language | Rust 1.96 (Edition 2024) |
| Async Runtime | Tokio |
| REST | Axum 0.8 |
| gRPC | Tonic 0.14 + Protobuf |
| Database | PostgreSQL 17 + pgvector + Redis 8 |
| VCS Engine | jj-lib 0.41.0 (GitBackend, multi-tenant) |
| Event Bus | Apache Kafka (KRaft, rdkafka) |
| Distributed Storage | IPFS Kubo (Merkle DAG IPLD) |
| AI Embeddings | fastembed (Nomic-Embed-Text-v1.5, ONNX 256d) |
| AI LLMs | Ollama (Granite3 2B + SmolLM2 1.7B) |
| Semantic Parsing | Tree-sitter 0.25 (Rust, TS, TSX, CSS, Python) |
| Frontend | Next.js 16 (Turbopack) + Bun 1.3.8 |
| Code Highlighting | Shiki 4.2 |
| Auth | JWT + PAT + GitHub OAuth + RBAC |
| Federation | ActivityPub + ForgeFed + HTTP Signatures (Draft-Cavage-12) |
| Observability | tracing + Prometheus + Grafana (17 panels) |
| Error Handling | thiserror (domain) + anyhow (binary) |

---

## 🔑 Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Hexagonal Architecture** | Domain stays pure — no framework deps. Adapters are swappable. |
| **jj-lib (not Git directly)** | First-class merge conflict support, anonymous branching, operation log |
| **Unified ticket counter** | Issues and MRs share `next_ticket_number` — `#ID` is unambiguous per repo (GitHub/GitLab convention) |
| **Nomic over MiniLM** | MTEB ~59.4 vs ~56.3, 8192 token context (16×), Matryoshka support |
| **ONNX local (not API)** | Zero network latency, zero cost, offline-capable |
| **pgvector HNSW** | No VACUUM required, consistent quality, O(log n) |
| **Dual Ollama instances** | Oracle (review) and Sensei (chat) run on separate ports — zero contention |
| **3-layer auth** | Public / Semi-Public / Private — Bouclier Global enforced at router level |
| **ActivityPub federation** | Decentralized, no vendor lock-in, interop with Mastodon/Forgejo |
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
- [x] **Phases 29–30** — AI Provenance gutter, UnifiedDiffViewer V2
- [x] **Phases 31–32** — Federation Dashboard (Makimono) + Inbox Worker (tokio)
- [x] **Phase 33** — Issues/Tickets (Le Parchemin des Doléances)
- [ ] **Phase 34** — Webhooks (CI/CD integrations)
- [ ] **Phase 28G** — Service Worker IPFS (decentralized browser resolution)
- [ ] **Phase 40** — AST-Level AI Provenance (Tree-sitter line-exact)

> 📜 Full architectural journal: [`CODEX_BOARD.md`](./CODEX_BOARD.md) (Vol. I) + [`CODEX_BOARD_1.md`](./CODEX_BOARD_1.md) (Vol. II — 3000+ lines)

---

## 📄 License

[MIT](LICENSE) — Copyright © 2026 YmClash
