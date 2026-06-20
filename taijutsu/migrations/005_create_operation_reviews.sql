-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- Migration 005: Table operation_reviews
-- Phase 9 — L'Oracle Reviewer (Agent Actif de Code Review)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Stocke les reviews de code produites par les agents IA (Oracle).
-- Chaque review est liée à une opération VCS via FK CASCADE.

CREATE TABLE IF NOT EXISTS operation_reviews (
    id              UUID PRIMARY KEY,
    operation_id    UUID NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    reviewer        TEXT NOT NULL,           -- Identifiant de l'agent (ex: 'oracle')
    model           TEXT NOT NULL,           -- Modèle LLM utilisé (ex: 'granite3.1-dense:2b')
    summary         TEXT NOT NULL,           -- Résumé court de la review
    content         TEXT NOT NULL,           -- Review complète en Markdown
    score           REAL,                    -- Score de qualité optionnel (0.0 à 1.0)
    duration_ms     BIGINT NOT NULL,         -- Temps de génération LLM en millisecondes
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour retrouver les reviews d'une opération (cas principal)
CREATE INDEX IF NOT EXISTS idx_reviews_operation ON operation_reviews(operation_id);

-- Index pour trier par date décroissante (reviews les plus récentes en premier)
CREATE INDEX IF NOT EXISTS idx_reviews_created ON operation_reviews(created_at DESC);
