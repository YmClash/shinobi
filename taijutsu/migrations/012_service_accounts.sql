-- ============================================================================
-- Phase 25 : L'Acte de Naissance — Service Accounts
-- ============================================================================
-- Ajoute la colonne `parent_id` à la table `actors` pour établir la lignée
-- entre un acteur humain (créateur) et ses agents IA (bots).
--
-- ON DELETE CASCADE : si l'humain est supprimé/purgé, tous ses bots sont
-- automatiquement désintégrés par PostgreSQL — pas d'orphelins.
-- ============================================================================

-- 1. Colonne parent_id avec clé étrangère auto-référentielle
ALTER TABLE actors
ADD COLUMN parent_id UUID REFERENCES actors(id) ON DELETE CASCADE;

-- 2. Index partiel pour les lookups bot→parent (seuls les bots ont un parent_id)
CREATE INDEX IF NOT EXISTS idx_actors_parent_id
ON actors (parent_id)
WHERE parent_id IS NOT NULL;

-- 3. Contrainte : seuls les AI agents peuvent avoir un parent_id
-- (les humains et le système n'ont pas de parent)
ALTER TABLE actors
ADD CONSTRAINT chk_parent_id_ai_only
CHECK (
    (actor_type = 'ai_agent' AND parent_id IS NOT NULL)
    OR (actor_type != 'ai_agent' AND parent_id IS NULL)
);
