-- ═══════════════════════════════════════════════════════════════
-- Migration 009 — GitHub OAuth (Phase 20)
--
-- Ajout du lien GitHub sur les acteurs :
--   - github_id (BIGINT UNIQUE) — identifiant GitHub immuable
--   - Index partiel pour les lookups OAuth rapides
-- ═══════════════════════════════════════════════════════════════

ALTER TABLE actors ADD COLUMN IF NOT EXISTS github_id BIGINT UNIQUE;

COMMENT ON COLUMN actors.github_id IS 'GitHub user ID — lien OAuth unique (Phase 20)';

CREATE INDEX IF NOT EXISTS idx_actors_github_id
    ON actors (github_id) WHERE github_id IS NOT NULL;
