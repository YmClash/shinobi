# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# SHINOBI / Taijutsu — Dockerfile Production
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#
# Build multi-stage :
#   Stage 1 (builder)  → Compile Rust release (~10 min cold build)
#   Stage 2 (runtime)  → Image slim Debian (~150 MB)
#
# Dépendances système (build) :
#   - cmake        : rdkafka (librdkafka, feature cmake-build)
#   - protobuf     : tonic-build (gRPC / Ninpo)
#   - pkg-config   : détection OpenSSL
#   - libssl-dev   : TLS (reqwest, sqlx, rdkafka)
#   - g++          : tree-sitter (grammaires C compilées)
#
# Usage :
#   docker build -t shinobi/taijutsu:latest .
#   docker run --rm shinobi/taijutsu:latest

# ── Stage 1 : Builder ─────────────────────────────
FROM rust:1.96-bookworm AS builder

# Dépendances système pour la compilation native
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copier les manifestes d'abord pour le cache de dépendances Docker
COPY Cargo.toml Cargo.lock ./
COPY crates/domain/Cargo.toml crates/domain/Cargo.toml
COPY crates/application/Cargo.toml crates/application/Cargo.toml
COPY crates/infrastructure/Cargo.toml crates/infrastructure/Cargo.toml
COPY crates/presentation/Cargo.toml crates/presentation/Cargo.toml
COPY crates/tensai/Cargo.toml crates/tensai/Cargo.toml

# Créer des fichiers lib.rs factices pour que Cargo résolve les dépendances
RUN mkdir -p src && echo "fn main() {}" > src/main.rs \
    && mkdir -p crates/domain/src && echo "" > crates/domain/src/lib.rs \
    && mkdir -p crates/application/src && echo "" > crates/application/src/lib.rs \
    && mkdir -p crates/infrastructure/src && echo "" > crates/infrastructure/src/lib.rs \
    && mkdir -p crates/presentation/src && echo "" > crates/presentation/src/lib.rs \
    && mkdir -p crates/tensai/src && echo "" > crates/tensai/src/lib.rs

# Proto files nécessaires pour tonic-build
COPY proto/ proto/
COPY crates/presentation/build.rs crates/presentation/build.rs

# Pré-compiler les dépendances (cache Docker layer)
RUN cargo build --release --bin taijutsu 2>/dev/null || true

# Copier le code source complet et recompiler
COPY . .

# Forcer la recompilation du code applicatif (pas les dépendances)
RUN touch src/main.rs \
    && touch crates/domain/src/lib.rs \
    && touch crates/application/src/lib.rs \
    && touch crates/infrastructure/src/lib.rs \
    && touch crates/presentation/src/lib.rs \
    && touch crates/tensai/src/lib.rs \
    && cargo build --release --bin taijutsu

# ── Stage 2 : Runtime ─────────────────────────────
FROM debian:bookworm-slim AS runtime

# Dépendances runtime minimales
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Binaire compilé
COPY --from=builder /app/target/release/taijutsu /usr/local/bin/taijutsu

# Migrations SQL (sqlx)
COPY --from=builder /app/migrations /app/migrations

# Cache fastembed ONNX (modèle Nomic ~80 MB) — immuable, zéro download runtime
COPY --from=builder /app/.fastembed_cache /app/.fastembed_cache

# Workspace VCS (volume en production)
RUN mkdir -p /app/workspace

WORKDIR /app

# Observabilité
ENV RUST_LOG=info,taijutsu=debug

# Ports : REST (3000) + gRPC (50051)
EXPOSE 3000 50051

# Health check Docker natif
HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
    CMD curl -sf http://localhost:3000/health || exit 1

ENTRYPOINT ["taijutsu"]
