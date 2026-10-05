-- ══════════════════════════════════════════════════════════════
-- Migration 022 — Notifications (Phase 38 — Le Carillon) 🔔
-- ══════════════════════════════════════════════════════════════
-- Table de notifications in-app pour les @mentions, reviews, etc.
-- Champs owner/repo dénormalisés pour éviter les N+1 queries.

CREATE TYPE notification_type AS ENUM (
    'mentioned', 'review_requested', 'review_received',
    'assigned', 'issue_closed', 'mr_merged'
);

CREATE TYPE notification_target_type AS ENUM ('issue', 'merge_request', 'comment');

CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    recipient_id UUID NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    actor_id UUID NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    notification_type notification_type NOT NULL,
    target_type notification_target_type NOT NULL,
    target_id UUID NOT NULL,
    target_number INT,
    repository_id UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    repository_owner TEXT NOT NULL DEFAULT '',
    repository_name TEXT NOT NULL DEFAULT '',
    message TEXT NOT NULL,
    read BOOLEAN NOT NULL DEFAULT FALSE,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour le listing par destinataire (query principale)
CREATE INDEX idx_notifications_recipient ON notifications(recipient_id, created_at DESC);

-- Index partiel pour le compteur de non-lues (O(1))
CREATE INDEX idx_notifications_unread ON notifications(recipient_id) WHERE read = FALSE;

-- Déduplication : une seule notification par (recipient, actor, type, target)
-- Empêche le spam si un commentaire est édité avec la même mention.
-- ON CONFLICT DO NOTHING dans l'adapter.
CREATE UNIQUE INDEX idx_notifications_dedup
    ON notifications(recipient_id, actor_id, notification_type, target_id);
