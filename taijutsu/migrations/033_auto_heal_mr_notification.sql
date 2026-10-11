-- ═══════════════════════════════════════════════════════════════════
-- Migration 033 — Notification « MR auto-fix Sensei » (Phase 42-pre-Fix) ⚔️🔔
-- ═══════════════════════════════════════════════════════════════════
-- Étend l'enum notification_type avec 'auto_heal_mr' : notifie le
-- propriétaire du dépôt (et l'auteur du push) quand le Kage Bunshin
-- ouvre une Merge Request de correctif.
ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'auto_heal_mr';
