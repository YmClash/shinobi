-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 020
-- Phase 37B : Fork Local (Le Dédoublement)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Ajoute le support du fork intra-instance :
--   forked_from_id — référence vers le dépôt parent
--   ON DELETE SET NULL — si le parent est supprimé, le fork survit
--   UNIQUE partiel (owner_id, forked_from_id) — un owner ne peut
--     forker qu'une seule fois le même dépôt source

-- ── Colonne forked_from_id ────────────────────────────────────────
ALTER TABLE repositories
    ADD COLUMN IF NOT EXISTS forked_from_id UUID REFERENCES repositories(id) ON DELETE SET NULL;

-- ── Index de lookup : « quels repos sont des forks de X ? » ──────
CREATE INDEX IF NOT EXISTS idx_repos_forked_from
    ON repositories (forked_from_id)
    WHERE forked_from_id IS NOT NULL;

-- ── Garde anti-doublon : un owner ne peut forker qu'une fois ─────
CREATE UNIQUE INDEX IF NOT EXISTS idx_repos_owner_fork_unique
    ON repositories (owner_id, forked_from_id)
    WHERE forked_from_id IS NOT NULL;
