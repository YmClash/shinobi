-- Phase 34-V3 — Levier Audit B2B (ANBU Webhooks)
-- Ajoute le type d'événement 'anbu_checkpoint' à l'enum webhook_event_type.
-- Permet aux clients B2B de s'abonner aux preuves de provenance IA.

ALTER TYPE webhook_event_type ADD VALUE IF NOT EXISTS 'anbu_checkpoint';
