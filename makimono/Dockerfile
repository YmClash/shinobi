# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# SHINOBI / Makimono — Dockerfile Production
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#
# Build multi-stage :
#   Stage 1 (deps)     → Installe les dépendances Bun (cache layer)
#   Stage 2 (builder)  → Build Next.js standalone
#   Stage 3 (runner)   → Node.js Alpine runtime (~100 MB)
#
# Le mode `output: "standalone"` de Next.js produit un serveur
# autonome sans node_modules (juste le strict nécessaire).
#
# Usage :
#   docker build -t shinobi/makimono:latest .
#   docker run --rm -p 3001:3001 shinobi/makimono:latest

# ── Stage 1 : Dependencies ────────────────────────
FROM oven/bun:1.3 AS deps

WORKDIR /app

COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

# ── Stage 2 : Build ───────────────────────────────
FROM oven/bun:1.3 AS builder

WORKDIR /app

COPY --from=deps /app/node_modules ./node_modules
COPY . .

# Désactiver la télémétrie Next.js
ENV NEXT_TELEMETRY_DISABLED=1

# URL interne du backend (réseau Docker shinobi)
ARG TAIJUTSU_URL=http://taijutsu:3000
ENV TAIJUTSU_URL=${TAIJUTSU_URL}

RUN bun run build

# ── Stage 3 : Runtime ─────────────────────────────
FROM node:22-alpine AS runner

WORKDIR /app

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1

# Next.js standalone écoute sur 0.0.0.0:3001
ENV HOSTNAME="0.0.0.0"
ENV PORT=3001

# Copier le serveur standalone Next.js
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
COPY --from=builder /app/public ./public

EXPOSE 3001

# Health check Docker natif
HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
    CMD wget -q --spider http://127.0.0.1:3001/ || exit 1

CMD ["node", "server.js"]
