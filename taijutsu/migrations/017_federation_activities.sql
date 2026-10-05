-- Migration 017 : Table des activités fédérées (Outbox ActivityPub).
--
-- Phase 27-bis-D — Stocke toutes les activités sortantes en JSONB
-- pour le polymorphisme natif d'ActivityPub (Create, Update, Delete, etc.).

CREATE TABLE IF NOT EXISTS federation_activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id UUID NOT NULL REFERENCES actors(id),
    activity_type TEXT NOT NULL,              -- "Create", "Update", "Delete", "Accept"
    object_type TEXT NOT NULL,                -- "Repository", "Note", "Follow"
    object_id TEXT NOT NULL,                  -- URI de l'objet (ex: https://domain/repos/owner/repo)
    activity_json JSONB NOT NULL,             -- Activité AP complète
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour l'outbox : tri chronologique par acteur
CREATE INDEX idx_fed_activities_actor ON federation_activities(actor_id, published_at DESC);
