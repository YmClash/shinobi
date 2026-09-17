-- ══════════════════════════════════════════════════════════════
-- Migration 026 — Webhooks (Phase 34 — Chakra チャクラ) 🔔
-- ══════════════════════════════════════════════════════════════
-- Système de webhooks HTTP pour l'intégration CI/CD.
-- Permet aux développeurs de brancher Drone CI, Woodpecker CI,
-- Jenkins en quelques clics.

-- Type ENUM pour les événements webhook
CREATE TYPE webhook_event_type AS ENUM (
    'push', 'mr_created', 'mr_merged', 'mr_closed',
    'issue_opened', 'issue_closed', 'issue_comment'
);

-- ── Table principale des webhooks ─────────────────────────────
CREATE TABLE webhooks (
    id UUID PRIMARY KEY,
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    creator_id UUID NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    secret TEXT NOT NULL,
    events webhook_event_type[] NOT NULL DEFAULT '{}',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    last_delivery_at TIMESTAMPTZ,
    failure_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour le lookup par repo (webhooks actifs uniquement).
-- C'est la query la plus fréquente : à chaque push, on cherche
-- tous les webhooks actifs du repo.
CREATE INDEX idx_webhooks_repo_active ON webhooks(repository_id) WHERE active = TRUE;

-- ── Historique des livraisons (audit trail) ───────────────────
CREATE TABLE webhook_deliveries (
    id UUID PRIMARY KEY,
    webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type webhook_event_type NOT NULL,
    event_id UUID NOT NULL,
    url TEXT NOT NULL,
    request_headers JSONB NOT NULL DEFAULT '{}',
    request_body TEXT NOT NULL DEFAULT '',
    response_status SMALLINT,
    response_body TEXT,
    response_headers JSONB,
    duration_ms INT,
    success BOOLEAN NOT NULL DEFAULT FALSE,
    attempt SMALLINT NOT NULL DEFAULT 1,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour le listing par webhook (query du dashboard)
CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries(webhook_id, created_at DESC);

-- Index pour le lookup par event_id (éviter les doublons de delivery)
CREATE INDEX idx_webhook_deliveries_event ON webhook_deliveries(event_id);
