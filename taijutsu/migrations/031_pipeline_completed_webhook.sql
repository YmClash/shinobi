-- ── Phase 42-pre-Fix — Kage Bunshin V2 🥷 ─────────────────────────────
-- Nouvel événement webhook émis à la fin de chaque pipeline Jutsu,
-- qu'il soit finalisé par le runner, le KageBunshinConsumer ou le watchdog.
-- (Même pattern que 027_anbu_webhook_event.sql.)

ALTER TYPE webhook_event_type ADD VALUE IF NOT EXISTS 'pipeline_completed';
