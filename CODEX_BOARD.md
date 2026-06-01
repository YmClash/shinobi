# ⚔️ SHINOBI — CODEX BOARD

> **Journal d'architecture et de progression du projet.**
> Mis à jour à chaque implémentation ou changement significatif.

---

## 📌 Informations Générales

| Champ | Valeur |
|---|---|
| **Projet** | SHINOBI — Next-Gen VCS pour collaboration Humain/IA |
| **Composant actif** | Taijutsu (Backend Core) |
| **Dernière mise à jour** | 2026-06-02 |
| **Rust** | 1.96.0 (stable) |
| **Edition** | 2024 |
| **Build status** | ✅ `cargo check` — OK |

---

## 🗺️ Architecture Globale du Système

```
┌──────────────────────────────────────────────────────────────────────────┐
│                        🌐 LE CHAMP DE BATAILLE                          │
│                                                                          │
│   💻 Humain                              🤖 Agent IA                    │
│   (Makimono UI)                          (Consommateur Tensai)           │
└───────────┬──────────────────────────────────────┬───────────────────────┘
            │                                      │
            ▼                                      ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                        🚪 PASSERELLE                                     │
│                     Reverse Proxy (Envoy)                                │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────────────┐
│              🧠 MOTEUR CENTRAL — TAIJUTSU [Rust + Axum + Tokio]         │
│                                                                          │
│  ┌──────────────────────┐   ┌───────────────────┐   ┌────────────────┐  │
│  │ ⚡ Serveur Ninpo      │   │ ⚙️  VCS Engine      │   │ ✂️  Tensai     │  │
│  │ (gRPC / Tonic)       │   │ (jj-lib)           │   │ (Tree-sitter) │  │
│  │ Port: 50051          │   │                    │   │               │  │
│  └──────────────────────┘   └───────────────────┘   └────────────────┘  │
│  ┌──────────────────────┐                                                │
│  │ 🌐 Serveur REST       │                                               │
│  │ (Axum)               │                                                │
│  │ Port: 3000           │                                                │
│  └──────────────────────┘                                                │
└────────────────────────────────────────────┬─────────────────────────────┘
                     │                       │
          ┌──────────┘                       └───────────┐
          ▼                                              ▼
┌─────────────────────────┐              ┌──────────────────────────────────┐
│ 💾 FŪINJUTSU            │              │ 🐝 GENJUTSU                      │
│ [PostgreSQL + Redis]    │              │ [IPFS + IPLD]                    │
│                         │              │                                  │
│ • Métadonnées ops VCS   │              │ • Nœud IPFS Serveur Central      │
│ • Verrous distribués    │              │ • Sync P2P ↔ Nœud local          │
│ • Cache applicatif      │              │ • Lecture locale (dév)           │
└─────────────────────────┘              └──────────────────────────────────┘
          │
          ▼
┌──────────────────────────────────────────────────────────────────────────┐
│ 📨 NEN — Apache Kafka                                                    │
│ Propagation événementielle async → Pipelines CI/CD                      │
└──────────────────────────────────────────────────────────────────────────┘
          │
          ▼
┌──────────────────────────────────────────────────────────────────────────┐
│ 👁️ DOJUTSU — Prometheus + Grafana                                        │
│ Observabilité silencieuse du système                                     │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 🧩 Table des Composants SHINOBI

| Composant | Nom de Code | Technologie | Statut |
|---|---|---|---|
| Frontend | **Makimono** | Bun + Next.js | 🔴 Non démarré |
| Backend Core | **Taijutsu** | Rust + Axum + Tokio | 🟡 Phase 1 — Fondations ✅ |
| Protocole gRPC | **Ninpo** | gRPC + Protobuf + Tonic | 🟡 Bootstrap ✅ |
| P2P & Stockage | **Genjutsu** | IPFS + IPLD | 🔴 Port défini, pas d'adaptateur |
| IA & Sémantique | **Tensai** | Tree-sitter | 🟡 Crate créé, stub |
| Base de Données | **Fūinjutsu** | PostgreSQL + Redis | 🟡 Adaptateurs stub/partiel |
| Événements | **Nen** | Apache Kafka | 🔴 Non démarré |
| Monitoring | **Dojutsu** | Prometheus + Grafana | 🔴 Non démarré |

---

## 🏗️ Architecture Hexagonale — Taijutsu Workspace

```
taijutsu/ (Cargo Workspace)
│
├── Cargo.toml                          # Workspace root + binary crate
├── Cargo.lock                          # Versions verrouillées
├── proto/
│   └── shinobi.proto                   # Définitions Protobuf (Ninpo)
├── src/
│   └── main.rs                         # Bootstrap: dual Axum:3000 + Tonic:50051
│
└── crates/
    │
    ├── domain/                         ← CŒUR PUR (0 dépendances techniques)
    │   └── src/
    │       ├── lib.rs
    │       ├── entities/
    │       │   ├── operation.rs        # Changement VCS atomique + merge support
    │       │   ├── content_id.rs       # CID IPLD — wrapper typé sur hash
    │       │   └── user.rs            # Acteur: Humain ou Agent IA
    │       ├── ports/
    │       │   ├── repository.rs       # Trait OperationRepository
    │       │   ├── vcs_engine.rs       # Trait VcsEngine (Anti-Corruption Layer)
    │       │   └── content_store.rs    # Trait ContentStore (→ Genjutsu)
    │       └── errors.rs              # DomainError (thiserror)
    │
    ├── application/                    ← USE CASES (orchestration pure)
    │   └── src/
    │       ├── lib.rs
    │       └── use_cases/
    │           └── create_operation.rs # Orchestre VcsEngine + OperationRepository
    │
    ├── infrastructure/                 ← ADAPTATEURS SECONDAIRES
    │   └── src/
    │       ├── lib.rs
    │       ├── persistence/
    │       │   └── postgres_repo.rs    # Fūinjutsu: impl OperationRepository (sqlx)
    │       ├── vcs/
    │       │   └── jujutsu_engine.rs   # jj-lib ACL: stub phase 1
    │       └── cache/
    │           └── redis_cache.rs      # Fūinjutsu: Redis locking + cache
    │
    ├── presentation/                   ← ADAPTATEURS PRIMAIRES (points d'entrée)
    │   ├── build.rs                    # Codegen Protobuf (tonic-prost-build)
    │   └── src/
    │       ├── lib.rs
    │       ├── rest/
    │       │   └── routes.rs          # Axum: GET /health, GET /api/v1/status
    │       └── grpc/
    │           └── services.rs        # Ninpo: ShinobiService (Ping, CreateOperation)
    │
    └── tensai/                         ← DÉCOUPAGE SÉMANTIQUE (crate indépendant)
        └── src/
            └── lib.rs                  # SemanticChunk, ChunkKind, Chunker trait
```

---

## 📦 Dépendances Workspace (Versions Réelles)

| Crate | Version | Rôle | Couche |
|---|---|---|---|
| `tokio` | 1.52.3 | Runtime async | Tous |
| `axum` | 0.8.9 | API REST | presentation |
| `tonic` | 0.14.6 | gRPC Ninpo | presentation |
| `tonic-prost-build` | 0.14.6 | Codegen Protobuf (build) | presentation |
| `tonic-prost` | 0.14.6 | Runtime codec gRPC | presentation |
| `prost` | 0.14.3 | Sérialisation Protobuf | presentation |
| `sqlx` | 0.9.0 | PostgreSQL async | infrastructure |
| `redis` | 1.2.2 | Cache + locking | infrastructure |
| `jj-lib` | 0.41 | Moteur VCS (phase 2) | infrastructure |
| `serde` | 1.0.228 | Sérialisation | domain, infra |
| `thiserror` | 2.0.18 | Erreurs typées | domain |
| `anyhow` | 1.0.102 | Erreurs contextuelles | binary |
| `tracing` | 0.1.44 | Observabilité structurée | tous |
| `tracing-subscriber` | 0.3.23 | Formatting logs | binary |
| `uuid` | 1.23.2 | Identifiants métier | domain, infra |
| `chrono` | 0.4.44 | Timestamps | domain, infra |
| `async-trait` | 0.1.89 | Traits async | domain, infra |

> **Outils installés :** `protoc` v35.0 (Protobuf compiler, via winget)

---

## 🔌 API Surface Actuelle

### REST — Axum (Port 3000)

| Méthode | Route | Description | Statut |
|---|---|---|---|
| `GET` | `/health` | Health check, retourne le statut JSON | ✅ Implémenté |
| `GET` | `/api/v1/status` | Status détaillé avec composants | ✅ Implémenté |

### gRPC Ninpo — Tonic (Port 50051)

| Service | RPC | Description | Statut |
|---|---|---|---|
| `ShinobiService` | `Ping` | Vérification disponibilité | ✅ Implémenté |
| `ShinobiService` | `CreateOperation` | Créer une opération VCS | 🟡 Stub (use case non connecté) |

---

## 📋 État Détaillé par Couche

### 🔴 Domain — `crates/domain/`
> **Status :** ✅ Complet pour Phase 1

| Élément | Type | Statut | Notes |
|---|---|---|---|
| `Operation` | Entité | ✅ | Immuable, merge support, IPLD-aware |
| `ContentId` | Value Object | ✅ | Wrapper typé, Display, From |
| `User` | Entité | ✅ | Humain ou IA — même traitement |
| `OperationRepository` | Port (Trait) | ✅ | save, find_by_id, list_recent, find_by_author |
| `VcsEngine` | Port (Trait) | ✅ | ACL contract: init, create, head, diff |
| `ContentStore` | Port (Trait) | ✅ | store, retrieve, exists, pin |
| `DomainError` | Erreur | ✅ | NotFound, BusinessRule, Conflict, Persistence, VcsError, StorageError, Internal |

### 🟡 Application — `crates/application/`
> **Status :** 🟡 Partiel — 1 use case sur N

| Élément | Type | Statut | Notes |
|---|---|---|---|
| `CreateOperationUseCase` | Use Case | ✅ | Injection via `Arc<dyn Trait>` |
| `ListOperationsUseCase` | Use Case | 🔴 | À implémenter en Phase 2 |
| `GetOperationUseCase` | Use Case | 🔴 | À implémenter en Phase 2 |
| `InitWorkspaceUseCase` | Use Case | 🔴 | À implémenter en Phase 2 |

### 🟡 Infrastructure — `crates/infrastructure/`
> **Status :** 🟡 Partiel — adaptateurs en stub

| Élément | Implémente | Statut | Notes |
|---|---|---|---|
| `PostgresOperationRepository` | `OperationRepository` | 🟡 | `save()` wired, reads → stub |
| `RedisCache` | (Cache/Locking) | ✅ | set, get, acquire_lock, release_lock (Lua) |
| `JujutsuEngine` | `VcsEngine` | 🟡 | Stub complet, roadmap Phase 2 inline |
| Adaptateur Genjutsu/IPFS | `ContentStore` | 🔴 | Port défini, aucun adaptateur |

### 🟡 Presentation — `crates/presentation/`
> **Status :** 🟡 Bootstrap OK, use cases non connectés

| Élément | Type | Statut | Notes |
|---|---|---|---|
| `create_router()` | Axum Router | ✅ | /health + /api/v1/status |
| `ShinobiServiceImpl` | Tonic Service | 🟡 | Ping OK, CreateOperation → stub |
| Protobuf codegen | Build script | ✅ | `tonic-prost-build` → `shinobi.ninpo` |
| AppState injection | DI Container | 🔴 | À implémenter en Phase 2 |

### 🟡 Tensai — `crates/tensai/`
> **Status :** 🟡 Fondations posées, logique à implémenter

| Élément | Type | Statut | Notes |
|---|---|---|---|
| `SemanticChunk` | Struct | ✅ | kind, name, content, lines, path, language |
| `ChunkKind` | Enum | ✅ | Function, Struct, Enum, Trait, Impl, Import, Module, Comment, Block |
| `Chunker` | Trait | ✅ | chunk(), supported_languages() |
| `ChunkerError` | Erreur | ✅ | UnsupportedLanguage, ParseError, Internal |
| Tree-sitter integration | Adaptateur | 🔴 | À implémenter en Phase 2 |

---

## 📝 Changelog

### `v0.1.0` — 2026-05-29 · *Phase 1 : Fondations*
> **Taijutsu workspace initialisé**

#### ✅ Réalisé
- Création du Cargo Workspace hexagonal (5 crates + binaire)
- Couche **Domain** complète : 3 entités, 3 ports, 1 type d'erreur
- Couche **Application** : use case `CreateOperation` avec injection `Arc<dyn>`
- Couche **Infrastructure** : adaptateurs PostgreSQL (save), Redis (locking complet), jj-lib (stub ACL)
- Couche **Presentation** : routeur Axum + service Tonic + codegen Protobuf
- Crate **Tensai** : fondations sémantiques (SemanticChunk, Chunker trait)
- `main.rs` : bootstrap dual-server avec banner ASCII, tracing, graceful shutdown
- `proto/shinobi.proto` : service Ninpo (Ping + CreateOperation)

#### 🐛 Corrections Build
| Problème | Cause | Solution |
|---|---|---|
| `axum = "0.12"` introuvable | Version inexistante (AI hallucination) | Corrigé → `0.8.9` |
| `sqlx 0.9` nécessite Rust 1.94 | Rust installé = 1.93 | `rustup update` → 1.96.0 |
| `tonic_build::compile_protos` introuvable | API changée en 0.14 | Migré vers `tonic-prost-build` |
| `tonic_prost::ProstCodec` introuvable | Crate runtime manquant | Ajout de `tonic-prost` en dépendance |
| Redis type inference `!` | Breaking change Rust 1.96 | Turbofish `::< _, _, ()>` |
| `protoc` manquant | Non installé sur le système | `winget install Google.Protobuf` v35.0 |
| File locks Windows (os error 32) | Antivirus/indexeur | `cargo clean` + build séquentiel |

---

## 🛣️ Roadmap

### Phase 2 — Intégration Réelle
- [ ] `jj-lib 0.41` : remplacement du stub ACL par l'implémentation réelle
- [ ] PostgreSQL : queries `find_by_id`, `list_recent`, `find_by_author`
- [ ] Schéma SQL : migrations `sqlx` pour la table `operations`
- [ ] AppState Axum : injection des use cases dans les handlers REST
- [ ] AppState Tonic : injection dans `ShinobiServiceImpl`
- [ ] `ListOperations` + `GetOperation` use cases
- [ ] Tree-sitter : premier chunker Rust dans le crate Tensai

### Phase 3 — Genjutsu & Nen
- [ ] Adaptateur IPFS/IPLD : impl `ContentStore`
- [ ] Kafka producer : publication d'événements VCS dans Nen
- [ ] Consumer CI/CD : déclenchement de pipelines sur événements

### Phase 4 — Dojutsu & Production
- [ ] Métriques Prometheus (`tokio-metrics`, `axum-prometheus`)
- [ ] Conteneurisation Docker (Taijutsu + PostgreSQL + Redis)
- [ ] Tests unitaires (domain + application)
- [ ] Tests d'intégration (infrastructure)
- [ ] TLS sur les serveurs Axum et Tonic

---

## 🔧 Commandes Utiles

```bash
# Compiler le workspace (depuis taijutsu/)
cargo check --workspace

# Lancer le serveur
cargo run

# Vérifier un crate spécifique
cargo check -p domain

# Voir l'arbre de dépendances
cargo tree --workspace

# Mettre à jour les dépendances
cargo update

# Lancer les tests (quand disponibles)
cargo test --workspace
```

---

*CODEX BOARD — SHINOBI Project · Mis à jour le 2026-06-02*
