-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 006
-- Phase 10A : Multi-Tenant (Actors, Repositories, Collaborators)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Modèle Actor-first aligné sur W3C ActivityPub :
--   actors (pas "users") — humains et IA = citoyens de première classe
--   credentials — auth découplée de l'identité (Phase 10B)
--   repositories — isolation multi-tenant du VCS
--   collaborators — rôles M:N actors ↔ repos
--
-- Le Fantôme : system actor + default repo pour rattacher les opérations MVP.

-- ── Actor Types ───────────────────────────────────────────────────────
DO $$ BEGIN
    CREATE TYPE actor_type AS ENUM ('human', 'ai_agent', 'system');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- ── Actors (pas "users" — alignement ActivityPub) ─────────────────────
CREATE TABLE IF NOT EXISTS actors (
    id           UUID        PRIMARY KEY,
    handle       TEXT        NOT NULL UNIQUE,
    display_name TEXT        NOT NULL,
    actor_type   actor_type  NOT NULL DEFAULT 'human',
    avatar_url   TEXT,
    bio          TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_actors_handle ON actors (handle);
CREATE INDEX IF NOT EXISTS idx_actors_type   ON actors (actor_type);

COMMENT ON TABLE actors IS 'Acteurs du système SHINOBI — humains, agents IA et système (modèle ActivityPub)';

-- ── Credentials (Auth Decoupling — Phase 10B) ─────────────────────────
-- Table prête pour la Phase 10B mais créée maintenant pour le schéma complet.
-- Un acteur peut avoir N credentials de types différents.
DO $$ BEGIN
    CREATE TYPE credential_type AS ENUM ('password', 'api_key', 'oauth_token', 'ssh_key');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS credentials (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id     UUID        NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    cred_type    credential_type NOT NULL,
    secret_hash  TEXT        NOT NULL,
    label        TEXT,
    email        TEXT,
    expires_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_credentials_actor ON credentials (actor_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_credentials_email
    ON credentials (email) WHERE email IS NOT NULL;

COMMENT ON TABLE credentials IS 'Credentials découplées — supporte password, API key, OAuth, SSH (Phase 10B)';

-- ── Repositories ──────────────────────────────────────────────────────
DO $$ BEGIN
    CREATE TYPE visibility AS ENUM ('public', 'private');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS repositories (
    id             UUID        PRIMARY KEY,
    owner_id       UUID        NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    name           TEXT        NOT NULL,
    display_name   TEXT        NOT NULL,
    description    TEXT,
    visibility     visibility  NOT NULL DEFAULT 'public',
    default_branch TEXT        NOT NULL DEFAULT 'main',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_repo_owner_name UNIQUE (owner_id, name)
);

CREATE INDEX IF NOT EXISTS idx_repos_owner     ON repositories (owner_id);
CREATE INDEX IF NOT EXISTS idx_repos_visibility ON repositories (visibility);

COMMENT ON TABLE repositories IS 'Dépôts de code versionné — isolation multi-tenant de la Forge Sociale';

-- ── Collaborators (M:N Actor ↔ Repository) ────────────────────────────
DO $$ BEGIN
    CREATE TYPE repo_role AS ENUM ('owner', 'maintainer', 'contributor', 'viewer');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS collaborators (
    actor_id      UUID      NOT NULL REFERENCES actors(id) ON DELETE CASCADE,
    repository_id UUID      NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    role          repo_role NOT NULL DEFAULT 'contributor',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (actor_id, repository_id)
);

COMMENT ON TABLE collaborators IS 'Relation M:N Actors ↔ Repositories avec rôles (owner, maintainer, contributor, viewer)';

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- LE FANTÔME — System Actor + Default Repository
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- UUIDs déterministes synchronisés avec les constantes Rust :
--   SYSTEM_ACTOR_ID = 00000000-0000-0000-0000-000000000001
--   DEFAULT_REPO_ID = 00000000-0000-0000-0000-000000000002

INSERT INTO actors (id, handle, display_name, actor_type)
VALUES ('00000000-0000-0000-0000-000000000001', 'system', 'SHINOBI System', 'system')
ON CONFLICT (id) DO NOTHING;

INSERT INTO repositories (id, owner_id, name, display_name, description)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'default',
    'Default Repository',
    'Dépôt par défaut — opérations MVP migrées automatiquement (Phase 10A)'
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO collaborators (actor_id, repository_id, role)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    '00000000-0000-0000-0000-000000000002',
    'owner'
)
ON CONFLICT DO NOTHING;

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- RATTACHEMENT DES OPÉRATIONS EXISTANTES
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- Étape 1 : Ajouter la colonne (nullable d'abord)
ALTER TABLE operations ADD COLUMN IF NOT EXISTS repository_id UUID;

-- Étape 2 : Rattacher toutes les opérations existantes au dépôt par défaut
UPDATE operations SET repository_id = '00000000-0000-0000-0000-000000000002'
WHERE repository_id IS NULL;

-- Étape 3 : Rendre la colonne NOT NULL (maintenant que toutes les lignes ont une valeur)
ALTER TABLE operations ALTER COLUMN repository_id SET NOT NULL;

-- Étape 4 : Ajouter la FK et l'index (idempotent)
DO $$ BEGIN
    ALTER TABLE operations ADD CONSTRAINT fk_operations_repository
        FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_operations_repo ON operations (repository_id);
