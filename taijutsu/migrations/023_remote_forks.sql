-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 023
-- Phase 37D : Fork Fédéré (La Diplomatie Décentralisée)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Trace les forks distants reçus via le protocole ActivityPub.
-- Quand une forge distante envoie un Offer(Fork) et qu'il est accepté,
-- une ligne est insérée ici. Le clone effectif est fait par la forge
-- distante via le Git Bridge HTTP — zero stockage Git local.
--
-- Contrainte UNIQUE (repository_id, remote_actor_uri) : un acteur
-- distant ne peut forker qu'une seule fois le même dépôt. Empêche
-- une instance buggée de spammer 500 forks identiques.

-- ── Table remote_forks ────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS remote_forks (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id    UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    remote_domain    TEXT NOT NULL,
    remote_actor_uri TEXT NOT NULL,
    remote_repo_url  TEXT,
    status           TEXT NOT NULL DEFAULT 'accepted',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Anti-spam : un acteur distant ne peut forker qu'une fois ─────
CREATE UNIQUE INDEX IF NOT EXISTS idx_remote_forks_unique
    ON remote_forks (repository_id, remote_actor_uri);

-- ── Lookup rapide : "quels forks distants pour ce repo ?" ────────
CREATE INDEX IF NOT EXISTS idx_remote_forks_repo
    ON remote_forks (repository_id);
