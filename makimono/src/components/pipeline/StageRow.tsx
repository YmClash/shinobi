"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Stage Row (Phase 40-C)
// Ligne d'un stage dans la timeline verticale des pipelines ⚡
// ═══════════════════════════════════════════════════════════════

import type { PipelineStage, PipelineStageStatus } from "@/lib/pipeline-api";

// ── Props ────────────────────────────────────────────────────

interface StageRowProps {
  /** Données du stage. */
  stage: PipelineStage;
  /** Dernier stage de la timeline (pas de connecteur après). */
  isLast: boolean;
  /** Callback pour ouvrir le LogDrawer sur ce stage. */
  onViewLogs: () => void;
}

// ── Status Config ────────────────────────────────────────────

const STAGE_STATUS_CONFIG: Record<PipelineStageStatus, { label: string; icon: string }> = {
  pending: { label: "En attente", icon: "⏳" },
  running: { label: "En cours",   icon: "🔄" },
  success: { label: "Succès",     icon: "✅" },
  failure: { label: "Échoué",     icon: "❌" },
  error:   { label: "Erreur",     icon: "⚠️" },
  skipped: { label: "Ignoré",     icon: "⏭️" },
};

// ── Component ────────────────────────────────────────────────

export function StageRow({ stage, isLast, onViewLogs }: StageRowProps) {
  const config = STAGE_STATUS_CONFIG[stage.status] ?? { label: stage.status, icon: "❓" };
  const durationStr = formatDuration(stage.duration_ms);

  return (
    <div className="pl-stage-row">
      {/* Connector column — dot + vertical line */}
      <div className="pl-stage-connector">
        <span className={`pl-stage-dot pl-stage-dot--${stage.status}`} />
        {!isLast && <span className="pl-stage-line" />}
      </div>

      {/* Info column */}
      <div className="pl-stage-info">
        <div className="pl-stage-header">
          <span className="pl-stage-name">{stage.name}</span>
          <span className="pl-stage-image">🐳 {stage.image}</span>
        </div>
        <div className="pl-stage-meta">
          <span>{config.icon} {config.label}</span>
          {durationStr !== "—" && <span>⏱ {durationStr}</span>}
          {stage.exit_code !== null && stage.exit_code !== 0 && (
            <span className="pl-exit-code">exit {stage.exit_code}</span>
          )}
        </div>
      </div>

      {/* Actions */}
      <div className="pl-stage-actions">
        {stage.logs && (
          <button
            className="pl-logs-btn"
            onClick={onViewLogs}
            title={`Voir les logs de ${stage.name}`}
          >
            📜 Logs
          </button>
        )}
      </div>
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────

function formatDuration(ms: number | null): string {
  if (ms === null) return "—";
  const s = Math.floor(ms / 1000);
  if (s < 1) return "< 1s";
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  if (m < 60) return `${m}m ${rs}s`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return `${h}h ${rm}m`;
}
