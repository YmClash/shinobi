-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 002
-- Phase 5A: Ajout du CID IPFS distribué (Genjutsu)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- Le CID IPFS est nullable : les opérations existantes n'en ont pas,
-- et les nouvelles opérations peuvent être créées sans IPFS
-- (graceful degradation si Kubo est indisponible).
ALTER TABLE operations ADD COLUMN IF NOT EXISTS ipfs_cid TEXT;

-- Index optionnel pour lookup par CID IPFS (phase 6 : résolution P2P)
CREATE INDEX IF NOT EXISTS idx_operations_ipfs_cid ON operations (ipfs_cid) WHERE ipfs_cid IS NOT NULL;

COMMENT ON COLUMN operations.ipfs_cid IS 'CID IPFS distribué (Genjutsu) — NULL si non synchronisé';
