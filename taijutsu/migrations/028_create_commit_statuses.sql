-- ══════════════════════════════════════════════════════════════
-- Migration 028 — Commit Statuses (Phase 39 — Le Pont CI/CD) 🌉
-- ══════════════════════════════════════════════════════════════
-- API de statuts de commit pour l'intégration CI/CD externe.
-- Permet aux pipelines Drone CI, Woodpecker, Jenkins de reporter
-- le résultat de leurs builds directement dans Shinobi.
--
-- Le champ `state` est un VARCHAR (pas un ENUM) pour supporter
-- l'évolution future des états sans migration DDL.
-- La validation stricte est faite au niveau applicatif.

CREATE TABLE commit_statuses (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id   UUID NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    commit_id       VARCHAR(64) NOT NULL,       -- SHA git (40) ou change ID jujutsu
    context         VARCHAR(255) NOT NULL,       -- ex: "drone/build", "woodpecker/test"
    state           VARCHAR(20) NOT NULL         -- pending | success | failure | error
                    CHECK (state IN ('pending', 'success', 'failure', 'error')),
    description     TEXT,                         -- "Build passed in 42s"
    target_url      TEXT,                         -- URL vers le dashboard CI externe
    creator_id      UUID REFERENCES actors(id),  -- Service Account ou acteur humain
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Un seul statut par contexte par commit (UPSERT key)
    UNIQUE(repository_id, commit_id, context)
);

-- Index pour le listing de tous les statuts d'un commit (query du badge UI).
-- C'est la query la plus fréquente : à chaque affichage d'un commit,
-- on récupère tous ses statuts pour calculer le combined status.
CREATE INDEX idx_commit_statuses_repo_commit ON commit_statuses(repository_id, commit_id);
