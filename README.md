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
  <em>Next-Generation Version Control System for Human/AI Collaboration</em>
  <br/><br/>
  <a href="#-quickstart"><img src="https://img.shields.io/badge/Rust-1.96+-orange?logo=rust" alt="Rust"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue" alt="License"></a>
  <a href="#-architecture"><img src="https://img.shields.io/badge/Architecture-Hexagonal-purple" alt="Architecture"></a>
  <a href="#-ai-powered-rag"><img src="https://img.shields.io/badge/AI-RAG%20Vectoriel-green" alt="AI"></a>
</p>

---

## 🥷 What is Shinobi?

**Shinobi** is an experimental, next-generation Version Control System designed from the ground up for **human and AI collaboration**. Unlike traditional VCS that treat code as flat text, Shinobi understands your code *semantically* — it extracts functions, structs, traits, and other constructs, vectorizes them using state-of-the-art embeddings, and enables **natural language search** over your entire codebase history.

### Key Capabilities

- **🧬 Semantic Code Memory** — Every code change is automatically chunked via Tree-sitter and vectorized via Nomic-Embed-Text-v1.5, enabling similarity search
- **🧠 AI Agent (Tensai)** — A real-time Kafka consumer that analyzes every VCS operation as it happens
- **🌐 Content-Addressable Storage** — Dual CID architecture: jj-lib (local VCS) + IPFS (distributed P2P)
- **⚡ Event-Driven Architecture** — Apache Kafka propagates mutations in real-time across the ecosystem
- **🔍 RAG Search** — Ask questions in natural language — Shinobi finds the relevant code

---

## 🏗️ Architecture

Shinobi follows **Hexagonal Architecture** (Ports & Adapters) with strict dependency inversion. The system is composed of named subsystems inspired by the ninja arts:

```
┌──────────────────────────────────────────────────────────────────┐
│              🧠 TAIJUTSU — Core Engine [Rust + Axum + Tokio]     │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐  │
│  │ ⚡ REST API   │  │ ⚙️  VCS Engine │  │ ✂️  Tensai (AI Agent)  │  │
│  │ (Axum :3000) │  │ (jj-lib)     │  │ Tree-sitter + Nomic   │  │
│  ├──────────────┤  └──────┬───────┘  │ + pgvector HNSW       │  │
│  │ 🌐 gRPC      │         │          └───────────┬────────────┘  │
│  │ (Tonic :50051│         │                      │               │
│  └──────────────┘         │                      │               │
└───────────────────────────┼──────────────────────┼───────────────┘
                            │                      │
          ┌─────────────────┼──────────────────────┼────────┐
          │                 │                      │        │
          ▼                 ▼                      ▼        ▼
   ┌────────────┐   ┌────────────┐        ┌──────────┐ ┌────────┐
   │ 💾 Fūinjutsu│   │ 🐝 Genjutsu │        │ 📨 Nen   │ │ 👁️ Dōjutsu│
   │ PostgreSQL │   │ IPFS/Kubo  │        │ Kafka    │ │ Prometheus│
   │ + pgvector │   │ P2P Storage│        │ KRaft    │ │ /metrics │
   │ + Redis    │   └────────────┘        └──────────┘ └────────┘
   └────────────┘
```

| Subsystem | Name | Technology | Purpose |
|-----------|------|------------|---------|
| Core Engine | **Taijutsu** | Rust, Axum, Tonic, Tokio | Backend — REST + gRPC servers, DI, bootstrap |
| VCS Engine | — | jj-lib 0.41 (SimpleBackend) | Atomic commits, tree building, diff |
| AI & Semantics | **Tensai** | Tree-sitter, Nomic, pgvector | Chunking, embedding, RAG search |
| Database | **Fūinjutsu** | PostgreSQL 17, Redis 8, pgvector | Persistence, caching, vector index |
| Storage | **Genjutsu** | IPFS Kubo | Content-addressable distributed storage |
| Events | **Nen** | Apache Kafka (KRaft) | Async event propagation |
| Monitoring | **Dōjutsu** | axum-prometheus | HTTP metrics (scrape endpoint) |

### Workspace Structure

```
shinobi/
├── README.md
├── LICENSE                         # MIT
├── CODEX_BOARD.md                  # Architecture journal (local)
├── .gitignore
│
└── taijutsu/                       # Cargo Workspace (5 crates + binary)
    ├── Cargo.toml                  # Workspace root
    ├── docker-compose.yml          # Dev services (PG + Redis + Kafka + IPFS)
    ├── .env / .env.example         # Configuration
    ├── proto/shinobi.proto         # gRPC schema (7 RPCs)
    ├── migrations/                 # 4 SQL migrations
    ├── src/
    │   ├── main.rs                 # Bootstrap + Backfill CLI
    │   └── config.rs               # Env vars loader
    │
    └── crates/
        ├── domain/                 # Pure domain (0 tech deps)
        │   └── entities, ports, errors
        ├── application/            # Use cases (orchestration)
        │   └── analyze, create, get, list, search
        ├── infrastructure/         # Adapters (PostgreSQL, Kafka, IPFS, jj-lib, ONNX)
        ├── presentation/           # REST + gRPC entry points
        └── tensai/                 # Semantic chunker (Tree-sitter AST)
```

---

## 🚀 Quickstart

### Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| **Rust** | 1.96+ | [rustup.rs](https://rustup.rs/) |
| **Docker** | 24+ | [docker.com](https://www.docker.com/) |
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
docker compose ps   # Verify all 4 services are healthy
```

This starts:
- **PostgreSQL** (pgvector) on port `5433`
- **Redis** on port `6380`
- **Apache Kafka** (KRaft) on port `9092`
- **IPFS Kubo** on port `5001`

### 3. Run Migrations

```bash
# Using psql directly (or sqlx-cli)
Get-Content migrations\001_create_operations.sql | docker exec -i shinobi-postgres psql -U shinobi -d shinobi_dev
Get-Content migrations\002_add_ipfs_cid.sql | docker exec -i shinobi-postgres psql -U shinobi -d shinobi_dev
Get-Content migrations\003_create_semantic_chunks.sql | docker exec -i shinobi-postgres psql -U shinobi -d shinobi_dev
Get-Content migrations\004_add_vector_embedding.sql | docker exec -i shinobi-postgres psql -U shinobi -d shinobi_dev
```

### 4. Build & Run

```bash
cargo run
```

On first launch, the Nomic-Embed-Text-v1.5 model (~522 MB) is downloaded and cached in `.fastembed_cache/`.

```
    ╔═══════════════════════════════════════════════════════════════╗
    ║   ███████╗██╗  ██╗██╗███╗   ██╗ ██████╗ ██████╗ ██╗         ║
    ║   ...                                                        ║
    ║   ⚙️  TAIJUTSU — Moteur Central v0.7.1                       ║
    ║   ⚡ Ninpo (gRPC) + Axum (REST) + Prometheus                 ║
    ║   🧬 RAG Vectoriel (Nomic-Embed-Text-v1.5 + pgvector)        ║
    ║   🧠 Tensai Agent IA — Boucle EDA complète                    ║
    ╚═══════════════════════════════════════════════════════════════╝
```

### 5. Verify

```bash
# Health check
curl http://localhost:3000/health

# System status
curl http://localhost:3000/api/v1/status
```

---

## 📡 API Reference

### REST API (Port 3000)

| Method | Route | Description |
|--------|-------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/api/v1/status` | System status with components |
| `POST` | `/api/v1/operations` | Create a VCS operation |
| `GET` | `/api/v1/operations` | List operations (`?limit=N&author_id=UUID`) |
| `GET` | `/api/v1/operations/{id}` | Get operation by ID |
| `GET` | `/api/v1/operations/{id}/chunks` | Get semantic chunks (`?file=path`) |
| `GET` | `/api/v1/chunks/search?name=X` | Search chunks by symbol name |
| `POST` | `/api/v1/chunks/semantic-search` | **RAG semantic search** |
| `GET` | `/metrics` | Prometheus metrics |

### gRPC — Ninpo Protocol (Port 50051)

| RPC | Description |
|-----|-------------|
| `Ping` | Availability check |
| `CreateOperation` | Create VCS operation |
| `GetOperation` | Get by ID |
| `ListOperations` | List with filters |
| `GetChunksByOperation` | Chunks for an operation |
| `SearchChunksByName` | Symbol search |
| `SemanticSearch` | **RAG vector search** |

---

## 🧬 AI-Powered RAG

Shinobi's AI agent (**Tensai**) provides real-time semantic analysis of your code:

### How It Works

```
1. Create Operation (with Rust files)
   └─► jj-lib commit + IPFS storage

2. Kafka publishes event (shinobi.vcs.operations)
   └─► Tensai consumer receives it

3. Tensai Analysis Pipeline:
   ├─ Download blob from IPFS
   ├─ Tree-sitter: extract functions, structs, traits, impls
   ├─ Nomic-Embed-Text-v1.5: vectorize each chunk (256d)
   ├─ PostgreSQL + pgvector: store with HNSW index
   └─ Kafka: publish analysis-complete event

4. Query with natural language
   └─► pgvector cosine similarity → ranked results
```

### Example: Semantic Search

```bash
curl -X POST http://localhost:3000/api/v1/chunks/semantic-search \
  -H "Content-Type: application/json" \
  -d '{"query": "user creation function", "limit": 5, "threshold": 0.3}'
```

Response:
```json
{
  "query": "user creation function",
  "count": 4,
  "chunks": [
    { "name": "User",  "kind": "struct",   "similarity": 0.603, "file_path": "src/lib.rs" },
    { "name": "User",  "kind": "impl",     "similarity": 0.600, "file_path": "src/lib.rs" },
    { "name": "new",   "kind": "function", "similarity": 0.587, "file_path": "src/lib.rs" },
    { "name": "main",  "kind": "function", "similarity": 0.558, "file_path": "src/main.rs" }
  ]
}
```

### Embedding Model

| Property | Value |
|----------|-------|
| Model | [Nomic-Embed-Text-v1.5](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5) |
| Runtime | ONNX (local CPU, no API calls) |
| Dimensions | 256 (Matryoshka truncation from 768) |
| Index | HNSW (pgvector, cosine distance) |
| Context | 8192 tokens |

### Backfill

Re-vectorize all existing operations:

```bash
cargo run -- backfill
```

The backfill is **idempotent** — existing chunks are deleted before re-insertion, so it can be run multiple times safely.

---

## 🧪 Testing

```bash
# Unit tests only (no Docker required)
cargo test --workspace

# Integration tests (Docker must be running)
cargo test --workspace -- --ignored

# All tests
cargo test --workspace -- --include-ignored
```

**Current coverage:** 56 tests passing (18 domain + 19 application + 17 infrastructure + 2 tensai)

---

## ⚙️ Configuration

All configuration is via environment variables (loaded from `.env`):

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | — | PostgreSQL connection string |
| `REDIS_URL` | — | Redis connection string |
| `REST_PORT` | `3000` | Axum HTTP server port |
| `GRPC_PORT` | `50051` | Tonic gRPC server port |
| `VCS_WORKSPACE_ROOT` | `./workspace` | jj-lib workspace path |
| `KAFKA_BROKERS` | `localhost:9092` | Kafka brokers |
| `KAFKA_TOPIC` | `shinobi.vcs.operations` | VCS events topic |
| `KAFKA_ANALYSIS_TOPIC` | `shinobi.tensai.analysis-complete` | Analysis events topic |
| `KAFKA_CONSUMER_GROUP` | `shinobi-tensai-analyzer` | Tensai consumer group |
| `TENSAI_CONSUMER_ENABLED` | `true` | Enable/disable AI agent |
| `IPFS_API_URL` | `http://127.0.0.1:5001` | IPFS Kubo RPC endpoint |
| `EMBEDDING_ENABLED` | `true` | Enable/disable RAG embeddings |
| `EMBEDDING_DIMENSIONS` | `256` | Embedding vector size (Matryoshka) |

---

## 🔑 Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Hexagonal Architecture** | Domain stays pure — no framework deps. Adapters are swappable. |
| **jj-lib (not Git)** | First-class merge conflict support, anonymous branching, operation log |
| **Nomic over MiniLM** | MTEB ~59.4 vs ~56.3, 8192 token context (16×), Matryoshka support |
| **ONNX local (not API)** | Zero network latency, zero cost, offline-capable, reproducible |
| **pgvector HNSW (not IVFFlat)** | No VACUUM required, consistent quality, O(log n) latency |
| **Kafka topic isolation** | Dedicated output topic prevents infinite consumer loops |
| **`delete_by_operation` before save** | Systemic idempotency — safe for backfill, Kafka replay, retries |
| **`Option<Arc<dyn T>>`** | Every external system is optional — graceful degradation everywhere |
| **Manual mocks (no framework)** | Zero compile-time overhead, total control, zero macro magic |

---

## 📦 Tech Stack

| Category | Technology |
|----------|------------|
| Language | Rust 1.96 (Edition 2024) |
| Async Runtime | Tokio |
| REST | Axum 0.8 |
| gRPC | Tonic 0.14 + Protobuf |
| Database | PostgreSQL 17 + pgvector + Redis 8 |
| VCS Engine | jj-lib 0.41.0 (SimpleBackend) |
| Event Bus | Apache Kafka (KRaft, rdkafka 0.39) |
| Distributed Storage | IPFS Kubo (reqwest) |
| AI Embeddings | fastembed 4.9 (Nomic-Embed-Text-v1.5, ONNX) |
| Semantic Parsing | Tree-sitter 0.25 |
| Observability | tracing + axum-prometheus |
| Error Handling | thiserror (domain) + anyhow (binary) |

---

## 🛣️ Roadmap

- [x] **Phase 1–3** — Foundations, hexagonal architecture, jj-lib transactional VCS
- [x] **Phase 4** — File writing, Kafka events, IPFS storage
- [x] **Phase 5** — Dual CID sync (jj ↔ IPFS), Prometheus, test coverage
- [x] **Phase 6** — AI Agent Tensai (consumer + chunker + persistence + API)
- [x] **Phase 7** — RAG vectoriel (Nomic + pgvector HNSW + EDA loop + backfill)
- [ ] **Phase 8** — Containerization, TLS, Grafana dashboards
- [ ] **Phase 9** — Multi-language support (Python, TypeScript, Go)
- [ ] **Phase 10** — Makimono (Frontend — Next.js/Bun)

---

## 📄 License

[MIT](LICENSE) — Copyright © 2026 YmClash
