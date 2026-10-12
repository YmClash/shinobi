-- Phase 42-pre-Fix — Opt-in de confidentialité IA par dépôt.
--
-- `allow_external_llm` : autorise l'envoi du code et des logs d'erreur à un
-- fournisseur LLM tiers (Gemini, Claude…) pour le Kage Bunshin.
--
-- Privacy by Design : DEFAULT FALSE → les nouveaux dépôts n'envoient rien
-- à l'extérieur tant que le propriétaire n'a pas activé l'option.
-- Les dépôts EXISTANTS sont activés pour ne pas casser les usages en cours
-- (ex : scénario E2E naruto/kunai-calc avec Gemini).

ALTER TABLE repositories
    ADD COLUMN IF NOT EXISTS allow_external_llm BOOLEAN NOT NULL DEFAULT FALSE;

-- Rattrapage : uniquement à l'ajout de la colonne (la migration ne rejoue pas).
UPDATE repositories SET allow_external_llm = TRUE;
