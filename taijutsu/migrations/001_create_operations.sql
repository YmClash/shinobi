-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 001
-- Schéma initial: table operations
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Table des opérations VCS (commits/changements atomiques).
-- Chaque opération est immuable : les corrections se font
-- par de nouvelles opérations pointant vers les parentes.
CREATE TABLE operations (
    id          UUID        PRIMARY KEY,
    author_id   UUID        NOT NULL,
    content_id  TEXT        NOT NULL,
    description TEXT        NOT NULL,
    parent_ids  JSONB       NOT NULL DEFAULT '[]',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Un même CID ne peut apparaître qu'une seule fois
    CONSTRAINT uq_operations_content_id UNIQUE (content_id)
);

-- Index pour les requêtes fréquentes
CREATE INDEX idx_operations_author  ON operations (author_id);
CREATE INDEX idx_operations_created ON operations (created_at DESC);
CREATE INDEX idx_operations_parent  ON operations USING GIN (parent_ids);

COMMENT ON TABLE operations IS 'Opérations VCS atomiques du graphe de versioning SHINOBI';
