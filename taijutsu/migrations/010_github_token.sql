-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 010
-- Phase 20B : GitHub Token Storage (Le Clonage Massif)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Stocke le token OAuth GitHub en clair pour permettre
-- les appels API GitHub authentifiés (list repos, etc.).
-- Le token est mis à jour à chaque login OAuth.
--

-- ── GitHub Token sur Actors ─────────────────────────────────────
-- Optionnel : seuls les acteurs ayant lié leur compte GitHub en ont un.
ALTER TABLE actors ADD COLUMN IF NOT EXISTS github_token TEXT;
