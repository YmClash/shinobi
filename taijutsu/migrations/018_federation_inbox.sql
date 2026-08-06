-- Migration 018 : Table Inbox fédéré (activités entrantes).
--
-- Phase 27-quater — Stocke les activités ActivityPub/ForgeFed reçues
-- d'autres forges fédérées via l'inbox S2S.
--
-- Séparation Outbox/Inbox :
--   - `federation_activities` (017) = activités SORTANTES (notre outbox)
--   - `federation_inbox` (018) = activités ENTRANTES (reçues de l'extérieur)
--
-- Pattern Transactional Inbox :
--   Les activités sont insérées avec `processed = false`.
--   Un worker futur (Phase 31+) traitera les side-effects
--   et passera `processed = true`.

CREATE TABLE IF NOT EXISTS federation_inbox (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Acteur local destinataire de l'activité
    recipient_actor_id UUID NOT NULL REFERENCES actors(id),

    -- Expéditeur distant (URI ActivityPub)
    remote_actor_uri TEXT NOT NULL,

    -- Métadonnées matérialisées (extraites du JSONB à l'insertion)
    -- Évite de parser le JSONB à chaque SELECT (pattern Materialized Columns)
    activity_type TEXT NOT NULL,              -- "Create", "Update", "Delete", "Push", "Announce"
    object_type TEXT NOT NULL DEFAULT '',     -- "Repository", "Note", "Commit", "Ticket"
    object_uri TEXT NOT NULL DEFAULT '',      -- URI de l'objet référencé

    -- Payload complet (source de vérité)
    activity_json JSONB NOT NULL,

    -- État de traitement (Transactional Inbox)
    processed BOOLEAN NOT NULL DEFAULT FALSE,
    processed_at TIMESTAMPTZ,

    -- Timestamps
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index principal : inbox d'un acteur, tri chronologique
CREATE INDEX idx_fed_inbox_recipient ON federation_inbox(recipient_actor_id, received_at DESC);

-- Index par type d'activité (dashboard Phase 31)
CREATE INDEX idx_fed_inbox_type ON federation_inbox(activity_type, received_at DESC);

-- Index partiel : activités non traitées (worker futur)
CREATE INDEX idx_fed_inbox_unprocessed ON federation_inbox(processed) WHERE processed = FALSE;

-- Déduplication : si un serveur distant bégaie, PostgreSQL rejette silencieusement.
-- L'ID ActivityPub est extrait du JSONB et forcé unique.
CREATE UNIQUE INDEX idx_fed_inbox_unique_activity_id
ON federation_inbox ((activity_json->>'id'))
WHERE activity_json->>'id' IS NOT NULL;
