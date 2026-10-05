-- Migration 014 : ANBU Checkpoints (Phase 28B)
--
-- Stocke les métadonnées des checkpoints ANBU synchronisés
-- depuis le CLI vers le serveur. Les artifacts eux-mêmes sont
-- sur IPFS (Genjutsu), référencés par le CID du manifeste DAG.

CREATE TABLE IF NOT EXISTS anbu_checkpoints (
    -- Identifiant unique du checkpoint (même UUID que le CLI local)
    id              UUID PRIMARY KEY,

    -- Repo cible (FK vers repositories)
    repository_id   UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,

    -- Acteur qui a synchronisé (FK vers actors)
    actor_id        UUID NOT NULL REFERENCES actors(id),

    -- Agent IA source (ex: 'antigravity')
    agent           TEXT NOT NULL,

    -- Session ID de l'agent (conversation UUID)
    session_id      TEXT NOT NULL,

    -- Message utilisateur
    message         TEXT,

    -- Référence jj (change-id ou commit hash)
    commit_id       TEXT,

    -- CID IPFS du manifeste DAG (racine des artifacts)
    ipfs_cid        TEXT NOT NULL,

    -- Nombre d'artifacts dans le checkpoint
    artifact_count  INTEGER NOT NULL DEFAULT 0,

    -- Taille totale des artifacts en bytes
    total_size      BIGINT NOT NULL DEFAULT 0,

    -- Timestamp de création
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index pour les requêtes fréquentes
CREATE INDEX idx_anbu_checkpoints_repo ON anbu_checkpoints(repository_id);
CREATE INDEX idx_anbu_checkpoints_session ON anbu_checkpoints(session_id);
CREATE INDEX idx_anbu_checkpoints_actor ON anbu_checkpoints(actor_id);
CREATE INDEX idx_anbu_checkpoints_created ON anbu_checkpoints(created_at DESC);
