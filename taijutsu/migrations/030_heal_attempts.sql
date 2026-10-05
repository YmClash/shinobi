-- ── Phase 41 — Kage Bunshin (影分身) · Auto-Healing CI/CD 🥷⚡ ──────
-- Table pour tracer les tentatives d'auto-correction par Sensei.
-- Chaque entrée représente une tentative de guérison d'un stage en échec.
--
-- Le flow complet :
--   Stage ❌ failure + kage_bunshin: true
--   → Sensei analyse les logs → patch en hunks (search/replace)
--   → Shadow workspace jj → re-run Docker → MR auto si succès
--
-- V1 : max 1 retry par stage, confidence ≥ 0.5, max 5 hunks.

CREATE TABLE IF NOT EXISTS heal_attempts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id     UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    stage_name      VARCHAR(255) NOT NULL,

    -- Diagnostic Sensei
    diagnosis       TEXT NOT NULL,                      -- Analyse LLM de l'erreur
    patch_summary   TEXT,                               -- Description humaine du fix
    hunks           JSONB NOT NULL DEFAULT '[]'::jsonb,  -- [{path, search, replace}]

    -- Résultat
    status          VARCHAR(20) NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'healing', 'success', 'failed')),
    shadow_branch   VARCHAR(255),                       -- "kage-bunshin/build/4d26b098"
    mr_id           UUID REFERENCES merge_requests(id), -- MR créée si success
    retry_logs      TEXT,                               -- Logs de la re-exécution shadow
    retry_exit_code SMALLINT,                           -- Exit code du re-run

    -- Métadonnées LLM
    llm_model       VARCHAR(100),                       -- "smollm2:1.7b"
    llm_duration_ms INTEGER,                            -- Temps d'inférence
    confidence      REAL,                               -- 0.0-1.0

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index : lookup par pipeline (dashboard UI)
CREATE INDEX idx_heal_attempts_pipeline ON heal_attempts(pipeline_id);

-- ALTER les CHECK constraints existantes pour ajouter les nouveaux statuts
-- Pipeline stages : ajouter 'healing' et 'healed'
ALTER TABLE pipeline_stages DROP CONSTRAINT IF EXISTS pipeline_stages_status_check;
ALTER TABLE pipeline_stages ADD CONSTRAINT pipeline_stages_status_check
    CHECK (status IN ('pending', 'running', 'success', 'failure', 'error', 'skipped', 'healing', 'healed'));
