-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 024
-- Phase 37E : Le Trou de Ver Git (Cross-Repo MR)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Étend le système de Merge Requests pour supporter les MR
-- cross-repo (fork → parent) — Le Trou de Ver Git.
--
-- source_repository_id : UUID du dépôt source (fork).
--   NULL = MR intra-repo (comportement Phase 26A préservé).
--   Renseigné = MR cross-repo (fork → parent).
--
-- La MR est numérotée dans le contexte du repo CIBLE (parent).
-- Le repository_id reste le repo cible (parent).
-- Le source_repository_id pointe vers le fork.
--
-- ON DELETE SET NULL : si le fork est supprimé, la MR survit
-- en tant que MR "orpheline" (le code est déjà mergé ou fermée).

-- ── Colonne source_repository_id ─────────────────────────────────
ALTER TABLE merge_requests
    ADD COLUMN IF NOT EXISTS source_repository_id UUID
    REFERENCES repositories(id) ON DELETE SET NULL;

-- ── Index : « quelles MR cross-repo existent pour ce fork ? » ────
CREATE INDEX IF NOT EXISTS idx_mr_source_repo
    ON merge_requests (source_repository_id)
    WHERE source_repository_id IS NOT NULL;

COMMENT ON COLUMN merge_requests.source_repository_id IS
    'UUID du dépôt source (fork) pour les MR cross-repo (Phase 37E). NULL = MR intra-repo.';
