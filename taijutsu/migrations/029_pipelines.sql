-- ── Phase 40 — Jutsu Runner Natif 🥷⚡ ──────────────────────────────
-- Tables pour les pipelines CI/CD natifs exécutés par le Jutsu Runner.
-- Granularité façon GitHub Actions : chaque stage a son statut + logs
-- indépendants. Le pipeline global agrège les résultats.

-- Table 1 : Pipelines (le parchemin global)
CREATE TABLE pipelines (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id   UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_id       VARCHAR(64) NOT NULL,
    trigger_event   VARCHAR(20) NOT NULL
                    CHECK (trigger_event IN ('push', 'mr_created', 'tag', 'manual')),
    status          VARCHAR(20) NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'running', 'success', 'failure', 'error', 'cancelled')),
    pipeline_name   TEXT,
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    duration_ms     INT,
    creator_id      UUID REFERENCES actors(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Table 2 : Pipeline Stages (chaque étape du parchemin)
CREATE TABLE pipeline_stages (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id     UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    image           TEXT NOT NULL,
    status          VARCHAR(20) NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'running', 'success', 'failure', 'error', 'skipped')),
    sort_order      SMALLINT NOT NULL DEFAULT 0,
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    duration_ms     INT,
    logs            TEXT,
    exit_code       SMALLINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index : lookup par repo + commit (dashboard pipelines)
CREATE INDEX idx_pipelines_repo_commit ON pipelines(repository_id, commit_id);

-- Index : lookup des stages d'un pipeline (ordre d'exécution)
CREATE INDEX idx_pipeline_stages_pipeline ON pipeline_stages(pipeline_id, sort_order);
