-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- SHINOBI / Fūinjutsu — Migration 007
-- Phase 19A : Auth Enhancements
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
--
-- Ajustements du schéma pour le système d'authentification :
--   - Colonne email optionnelle sur actors (login humain)
--   - Index sur credentials.secret_hash pour la recherche PAT
--

-- ── Email sur Actors (login Phase 19A) ───────────────────────
-- Optionnel car les agents IA et le system actor n'ont pas d'email.
ALTER TABLE actors ADD COLUMN IF NOT EXISTS email TEXT;

-- Email unique parmi les acteurs qui en ont un.
CREATE UNIQUE INDEX IF NOT EXISTS idx_actors_email
    ON actors (email) WHERE email IS NOT NULL;

-- ── Index PAT pour authentification Git HTTP ──────────────────
-- Optimise la recherche de credentials de type 'api_key' par leur hash.
CREATE INDEX IF NOT EXISTS idx_credentials_secret_hash
    ON credentials (secret_hash) WHERE cred_type = 'api_key';
