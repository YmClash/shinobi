-- ══════════════════════════════════════════════════════════════
-- Migration 025 — Add mr_opened notification type (Phase 37E-UI)
-- ══════════════════════════════════════════════════════════════
-- Étend l'enum notification_type avec 'mr_opened' pour notifier
-- le owner du repo parent quand une MR cross-repo est créée.

ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'mr_opened';
