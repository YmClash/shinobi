-- Phase 27 — ForgeFed: Follows fédérés (relations inter-instances)
--
-- Stocke les relations Follow ActivityPub entre des acteurs distants
-- (identifiés par leur URI ActivityPub) et des acteurs locaux.

CREATE TABLE IF NOT EXISTS federation_follows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    follower_uri TEXT NOT NULL,                              -- URI AP du follower distant
    following_actor_id UUID NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    accepted BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(follower_uri, following_actor_id)
);

-- Index pour lister les followers d'un acteur local
CREATE INDEX IF NOT EXISTS idx_federation_follows_actor ON federation_follows(following_actor_id);
