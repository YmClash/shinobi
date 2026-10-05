-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 011
-- Phase 24 : Soft Delete — Corbeille avec rétention
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Ajoute la colonne `deleted_at` aux repositories.
-- NULL = dépôt actif, NOT NULL = en corbeille (soft-deleted).
-- Les repos soft-deleted sont automatiquement exclus des listings
-- par les requêtes filtrées `WHERE deleted_at IS NULL`.
--
-- Purge automatique : un timer background supprime définitivement
-- les repos dont `deleted_at` est plus ancien que le délai de rétention.

ALTER TABLE repositories ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

-- Index partiel pour retrouver rapidement les repos à purger
CREATE INDEX IF NOT EXISTS idx_repos_deleted_at
    ON repositories (deleted_at) WHERE deleted_at IS NOT NULL;

COMMENT ON COLUMN repositories.deleted_at IS
    'NULL = actif, NOT NULL = en corbeille (soft-deleted). Purge automatique après le délai de rétention.';
