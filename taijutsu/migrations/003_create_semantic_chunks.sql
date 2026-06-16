-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 003
-- Phase 6B: Table semantic_chunks (Mémoire IA)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- Table des fragments sémantiques extraits par Tensai (Tree-sitter).
-- Chaque chunk est lié à l'opération VCS qui l'a produit.
-- La relation ON DELETE CASCADE assure la cohérence :
-- si une opération est supprimée, ses chunks disparaissent aussi.
CREATE TABLE IF NOT EXISTS semantic_chunks (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    operation_id  UUID        NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
    kind          TEXT        NOT NULL,   -- 'function', 'struct', 'enum', 'trait', 'impl', etc.
    name          TEXT,                    -- Nullable (certains blocs n'ont pas de nom)
    content       TEXT        NOT NULL,
    start_line    INT         NOT NULL,
    end_line      INT         NOT NULL,
    file_path     TEXT        NOT NULL,
    language      TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour les requêtes fréquentes de l'agent IA.

-- Lookup principal : "tous les chunks de cette opération"
CREATE INDEX IF NOT EXISTS idx_chunks_operation ON semantic_chunks (operation_id);

-- Recherche par fichier : "quels symboles dans src/main.rs ?"
CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON semantic_chunks (file_path);

-- Filtrage par type : "toutes les fonctions du projet"
CREATE INDEX IF NOT EXISTS idx_chunks_kind ON semantic_chunks (kind);

-- Recherche par symbole : "où est la struct User ?"
CREATE INDEX IF NOT EXISTS idx_chunks_name ON semantic_chunks (name) WHERE name IS NOT NULL;

COMMENT ON TABLE semantic_chunks IS 'Fragments sémantiques extraits par Tensai (Tree-sitter) — Mémoire permanente de l''agent IA';
COMMENT ON COLUMN semantic_chunks.kind IS 'Type de fragment: function, struct, enum, trait, impl, import, module, comment, block, unknown';
COMMENT ON COLUMN semantic_chunks.operation_id IS 'FK vers l''opération VCS source — CASCADE à la suppression';
