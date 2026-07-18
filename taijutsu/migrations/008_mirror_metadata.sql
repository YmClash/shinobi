-- ═══════════════════════════════════════════════════════════════
-- Phase 19B — Le Pont des Mondes (GitHub Repo Import)
-- ═══════════════════════════════════════════════════════════════
-- Ajoute les colonnes de mirroring à la table repositories.
-- Un repo natif SHINOBI a mirror_source_url = NULL.
-- Un repo importé depuis GitHub a l'URL source + la date du dernier sync.

ALTER TABLE repositories ADD COLUMN IF NOT EXISTS mirror_source_url TEXT;
ALTER TABLE repositories ADD COLUMN IF NOT EXISTS mirror_synced_at  TIMESTAMPTZ;

COMMENT ON COLUMN repositories.mirror_source_url IS
  'URL Git source pour les repos importés (ex: https://github.com/user/repo.git). NULL = repo natif SHINOBI.';
COMMENT ON COLUMN repositories.mirror_synced_at IS
  'Timestamp du dernier import miroir réussi. NULL = jamais synchronisé ou repo natif.';
