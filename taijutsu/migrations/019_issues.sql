-- ============================================================================
-- Phase 33 : Le Parchemin des Doléances — Issues/Tickets
-- ============================================================================
-- Système de suivi d'issues pour la collaboration :
--   issues              — entité centrale (title, body, status, assignee)
--   issue_comments      — commentaires Markdown
--   issue_events        — timeline d'audit (opened, closed, commented, etc.)
--   issue_labels        — étiquettes colorées scopées par repo
--   issue_label_assignments — pivot M:N issues <-> labels
--
-- ## Compteur Unifié
-- Le champ `next_ticket_number` est ajouté à `repo_counters` et remplace
-- `next_mr_number` pour les deux systèmes (Issues + MRs). Garantit que
-- `#ID` est non ambigu au sein d'un dépôt (convention GitHub/GitLab).
-- ============================================================================

-- ── Enums ───────────────────────────────────────────────────────────────

DO $$ BEGIN
    CREATE TYPE issue_status AS ENUM ('open', 'closed');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
    CREATE TYPE issue_event_type AS ENUM (
        'opened', 'closed', 'reopened', 'commented',
        'title_changed', 'body_changed',
        'label_added', 'label_removed',
        'assigned', 'unassigned'
    );
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ── Compteur unifié (Issues + MRs partagent le même #ID) ───────────────
-- Ajoute `next_ticket_number` et initialise à la valeur courante de
-- `next_mr_number` pour éviter les collisions avec les MRs existantes.

ALTER TABLE repo_counters
    ADD COLUMN IF NOT EXISTS next_ticket_number INT NOT NULL DEFAULT 1;

-- Migrer les compteurs existants : le compteur unifié doit commencer
-- au MAX des compteurs MR existants pour chaque repo.
UPDATE repo_counters
SET next_ticket_number = next_mr_number
WHERE next_mr_number > next_ticket_number;

-- ── Issues ──────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS issues (
    id             UUID         PRIMARY KEY,
    repository_id  UUID         NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    author_id      UUID         NOT NULL REFERENCES actors(id),
    number         INT          NOT NULL,
    title          TEXT         NOT NULL,
    body           TEXT,
    status         issue_status NOT NULL DEFAULT 'open',
    assignee_id    UUID         REFERENCES actors(id),
    closed_by      UUID         REFERENCES actors(id),
    closed_at      TIMESTAMPTZ,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_issue_repo_number UNIQUE (repository_id, number)
);

CREATE INDEX IF NOT EXISTS idx_issue_repo_status ON issues (repository_id, status);
CREATE INDEX IF NOT EXISTS idx_issue_repo_number ON issues (repository_id, number);
CREATE INDEX IF NOT EXISTS idx_issue_author ON issues (author_id);

COMMENT ON TABLE issues IS 'Issues/Tickets — système de suivi (Phase 33 — Le Parchemin des Doléances)';

-- ── Issue Labels (scopés par repository) ────────────────────────────────

CREATE TABLE IF NOT EXISTS issue_labels (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    color         TEXT NOT NULL DEFAULT '#6b7280',
    description   TEXT,

    CONSTRAINT uq_label_repo_name UNIQUE (repository_id, name)
);

CREATE INDEX IF NOT EXISTS idx_issue_labels_repo ON issue_labels (repository_id);

COMMENT ON TABLE issue_labels IS 'Labels (étiquettes colorées) scopés par dépôt (Phase 33)';

-- ── Pivot table Issues <-> Labels (M:N) ─────────────────────────────────

CREATE TABLE IF NOT EXISTS issue_label_assignments (
    issue_id UUID NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES issue_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id, label_id)
);

COMMENT ON TABLE issue_label_assignments IS 'Assignation M:N issues <-> labels (Phase 33)';

-- ── Issue Comments ──────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS issue_comments (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id   UUID        NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    author_id  UUID        NOT NULL REFERENCES actors(id),
    body       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_issue_comments_issue ON issue_comments (issue_id);

COMMENT ON TABLE issue_comments IS 'Commentaires Markdown sur les issues (Phase 33)';

-- ── Issue Events (Timeline / Audit Trail) ───────────────────────────────

CREATE TABLE IF NOT EXISTS issue_events (
    id         UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    issue_id   UUID             NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    actor_id   UUID             NOT NULL REFERENCES actors(id),
    event_type issue_event_type NOT NULL,
    payload    JSONB            DEFAULT '{}',
    created_at TIMESTAMPTZ      NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_issue_events_issue ON issue_events (issue_id);

COMMENT ON TABLE issue_events IS 'Timeline des événements issue — audit trail (Phase 33)';
