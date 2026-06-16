-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 004
-- Phase 7A: Évolution Vectorielle (pgvector + Nomic embeddings)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- Activer l'extension pgvector.
CREATE EXTENSION IF NOT EXISTS vector;

-- Colonne embedding (256 dimensions = Nomic-Embed-Text-v1.5 Matryoshka).
-- Nullable : les chunks existants (Phase 6B) n'ont pas d'embedding.
ALTER TABLE semantic_chunks
    ADD COLUMN IF NOT EXISTS embedding vector(256);

-- Index HNSW pour la recherche par similarité cosinus.
-- HNSW > IVFFlat : pas de VACUUM requis, qualité constante,
-- latence prévisible, meilleur recall pour les petits datasets.
CREATE INDEX IF NOT EXISTS idx_chunks_embedding
    ON semantic_chunks USING hnsw (embedding vector_cosine_ops);
