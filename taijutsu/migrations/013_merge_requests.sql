-- ============================================================================
-- Phase 26A : Le Katana Croisé — Merge Requests
-- ============================================================================
-- Système de Merge Requests pour la collaboration :
--   merge_requests  — entité centrale (source/target branch, status, reviews)
--   repo_counters   — compteur atomique anti race-condition pour la numérotation
--   mr_reviews      — reviews globales (Approve / Changes Requested)
--   mr_events       — timeline d'audit (opened, merged, closed, reviewed, etc.)
--
-- ForgeFed-Ready : le modèle est aligné sur le vocabulaire ForgeFed
-- (PullRequest, Repository, Actor) pour la fédération Phase 27.
-- ============================================================================

-- ── Enums ───────────────────────────────────────────────────────────────

DO $$ BEGIN
    CREATE TYPE mr_status AS ENUM ('open', 'merged', 'closed');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE mr_verdict AS ENUM ('approve', 'changes_requested');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE mr_event_type AS ENUM (
        'opened', 'pushed', 'reviewed', 'approved',
        'changes_requested', 'merged', 'closed', 'reopened'
    );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ── Compteur atomique par repo (anti race-condition) ────────────────────
-- Utilisation : UPDATE repo_counters SET next_mr_number = next_mr_number + 1
--              WHERE repository_id = $1 RETURNING next_mr_number - 1
-- Le premier INSERT est un UPSERT (ON CONFLICT DO UPDATE) pour auto-créer
-- la ligne lors de la première MR d'un repo.

CREATE TABLE IF NOT EXISTS repo_counters (
    repository_id UUID PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
    next_mr_number INT NOT NULL DEFAULT 1
);

COMMENT ON TABLE repo_counters IS 'Compteurs atomiques par dépôt — numérotation séquentielle des MR (Phase 26A)';

-- ── Merge Requests ──────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS merge_requests (
    id             UUID        PRIMARY KEY,
    repository_id  UUID        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    author_id      UUID        NOT NULL REFERENCES actors(id),
    number         INT         NOT NULL,
    title          TEXT        NOT NULL,
    description    TEXT,
    source_branch  TEXT        NOT NULL,
    target_branch  TEXT        NOT NULL DEFAULT 'main',
    status         mr_status   NOT NULL DEFAULT 'open',
    merged_by      UUID        REFERENCES actors(id),
    merged_at      TIMESTAMPTZ,
    closed_at      TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_mr_repo_number UNIQUE (repository_id, number)
);

CREATE INDEX IF NOT EXISTS idx_mr_repo_status ON merge_requests (repository_id, status);
CREATE INDEX IF NOT EXISTS idx_mr_repo_number ON merge_requests (repository_id, number);
CREATE INDEX IF NOT EXISTS idx_mr_author ON merge_requests (author_id);

COMMENT ON TABLE merge_requests IS 'Merge Requests — collaboration branches (Phase 26A — Le Katana Croisé)';

-- ── MR Reviews ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS mr_reviews (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    mr_id        UUID        NOT NULL REFERENCES merge_requests(id) ON DELETE CASCADE,
    reviewer_id  UUID        NOT NULL REFERENCES actors(id),
    verdict      mr_verdict  NOT NULL,
    body         TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_mr_reviews_mr ON mr_reviews (mr_id);

COMMENT ON TABLE mr_reviews IS 'Reviews de MR — Approve ou Changes Requested (Phase 26A)';

-- ── MR Events (Timeline / Audit Trail) ──────────────────────────────────

CREATE TABLE IF NOT EXISTS mr_events (
    id          UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    mr_id       UUID          NOT NULL REFERENCES merge_requests(id) ON DELETE CASCADE,
    actor_id    UUID          NOT NULL REFERENCES actors(id),
    event_type  mr_event_type NOT NULL,
    payload     JSONB         DEFAULT '{}',
    created_at  TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_mr_events_mr ON mr_events (mr_id);

COMMENT ON TABLE mr_events IS 'Timeline des événements MR — audit trail (Phase 26A)';
