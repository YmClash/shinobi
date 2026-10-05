-- Phase 37C — Le Mégaphone (@Mentions)
-- Ajouter le type d'événement 'mentioned' aux enums de timeline
-- Permet de stocker les notifications de mention dans IssueEvent et MrEvent

ALTER TYPE issue_event_type ADD VALUE IF NOT EXISTS 'mentioned';
ALTER TYPE mr_event_type ADD VALUE IF NOT EXISTS 'mentioned';
