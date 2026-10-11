-- ── Phase 42-pre-Fix — Idempotence du JutsuConsumer 🥷 ─────────────────
-- Garantie BDD (atomique, sans fenêtre temporelle) : un seul pipeline
-- par (dépôt, commit, déclencheur) pour les triggers automatiques.
--
-- Les triggers `manual` sont exclus : relancer manuellement un pipeline
-- sur le même commit est un usage légitime (bouton UI / `anbu jutsu trigger`).
--
-- Vérifié avant création : aucun doublon non-manuel en base au 2026-10-11.

CREATE UNIQUE INDEX IF NOT EXISTS uq_pipelines_repo_commit_trigger_auto
    ON pipelines (repository_id, commit_id, trigger_event)
    WHERE trigger_event <> 'manual';
