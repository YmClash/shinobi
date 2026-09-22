"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Pipeline Detail (Phase 40-C)
// Vue détaillée d'un pipeline : header + timeline stages ⚡
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import Link from "next/link";
import { usePipelineDetail, usePipelineStages } from "@/hooks/use-pipelines";
import { StageRow } from "./StageRow";
import { LogDrawer } from "./LogDrawer";
import type {
  PipelineStage,
  PipelineStatus,
  TriggerEvent,
} from "@/lib/pipeline-api";

// ── Status & Trigger Config ─────────────────────────────────

const PIPELINE_STATUS: Record<PipelineStatus, { label: string; icon: string }> = {
  queued:    { label: "En file",   icon: "⏳" },
  running:   { label: "En cours",  icon: "🔄" },
  success:   { label: "Succès",    icon: "✅" },
  failure:   { label: "Échoué",    icon: "❌" },
  error:     { label: "Erreur",    icon: "⚠️" },
  cancelled: { label: "Annulé",    icon: "🚫" },
};

const TRIGGER_LABELS: Record<TriggerEvent, string> = {
  push: "📤 push",
  manual: "👆 manual",
  mr_created: "⚔️ MR créé",
  mr_merged: "🔀 MR mergé",
};

// ── Props ────────────────────────────────────────────────────

interface PipelineDetailProps {
  owner: string;
  repo: string;
  id: string;
}

// ── Component ────────────────────────────────────────────────

export function PipelineDetail({ owner, repo, id }: PipelineDetailProps) {
  const { data: detail, loading, error } = usePipelineDetail(owner, repo, id);

  // Live polling des stages (3s quand running/queued)
  const { data: liveStages } = usePipelineStages(
    owner,
    repo,
    id,
    detail?.status ?? "",
  );

  // State pour le LogDrawer
  const [selectedStage, setSelectedStage] = useState<PipelineStage | null>(null);

  // Stages à afficher : les polled (plus frais) ou ceux du detail
  const displayStages = (liveStages ?? detail?.stages ?? [])
    .slice()
    .sort((a, b) => a.sort_order - b.sort_order);

  // ── Loading ─────────────────────────────────────────────────
  if (loading && !detail) {
    return (
      <div className="pl-page">
        <Link href={`/${owner}/${repo}/jutsus`} className="pl-back-link">
          ← Retour aux Jutsus
        </Link>
        <div className="pl-skeleton">
          <div className="pl-skeleton-row" style={{ height: 120 }} />
          <div className="pl-skeleton-row" style={{ height: 80 }} />
          <div className="pl-skeleton-row" style={{ height: 80 }} />
        </div>
      </div>
    );
  }

  // ── Error ───────────────────────────────────────────────────
  if (error && !detail) {
    return (
      <div className="pl-page">
        <Link href={`/${owner}/${repo}/jutsus`} className="pl-back-link">
          ← Retour aux Jutsus
        </Link>
        <div className="pl-empty">
          <div className="pl-empty-icon">⚠️</div>
          <p>{error}</p>
        </div>
      </div>
    );
  }

  if (!detail) return null;

  const config = PIPELINE_STATUS[detail.status] ?? { label: detail.status, icon: "❓" };
  const triggerStr = TRIGGER_LABELS[detail.trigger_event] ?? detail.trigger_event;

  return (
    <div className="pl-page">
      {/* Back link */}
      <Link href={`/${owner}/${repo}/jutsus`} className="pl-back-link">
        ← Retour aux Jutsus
      </Link>

      {/* Detail header */}
      <div className="pl-detail-header">
        <div className="pl-detail-top">
          <span className={`pl-status-dot pl-status-dot--${detail.status}`} />
          <span className="pl-detail-name">
            {detail.pipeline_name ?? "Pipeline"}
          </span>
          <span className={`pl-detail-status pl-detail-status--${detail.status}`}>
            {config.icon} {config.label}
          </span>
        </div>

        <div className="pl-detail-grid">
          <span className="pl-detail-label">Commit</span>
          <span className="pl-detail-value">{detail.commit_id.slice(0, 12)}</span>

          <span className="pl-detail-label">Trigger</span>
          <span className="pl-detail-value">{triggerStr}</span>

          <span className="pl-detail-label">Durée</span>
          <span className="pl-detail-value">{formatDuration(detail.duration_ms)}</span>

          {detail.started_at && (
            <>
              <span className="pl-detail-label">Démarré</span>
              <span className="pl-detail-value">{formatDateTime(detail.started_at)}</span>
            </>
          )}

          {detail.finished_at && (
            <>
              <span className="pl-detail-label">Terminé</span>
              <span className="pl-detail-value">{formatDateTime(detail.finished_at)}</span>
            </>
          )}
        </div>
      </div>

      {/* Stages timeline */}
      <div className="pl-timeline-title">
        🔗 Stages ({displayStages.length})
      </div>

      {displayStages.length === 0 ? (
        <div className="pl-empty">
          <p>Aucun stage défini pour ce pipeline.</p>
        </div>
      ) : (
        <div className="pl-timeline">
          {displayStages.map((stage, i) => (
            <StageRow
              key={stage.id}
              stage={stage}
              isLast={i === displayStages.length - 1}
              onViewLogs={() => setSelectedStage(stage)}
            />
          ))}
        </div>
      )}

      {/* Log Drawer */}
      <LogDrawer
        stage={selectedStage}
        open={!!selectedStage}
        onClose={() => setSelectedStage(null)}
      />
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

function formatDateTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString("fr-FR", {
      day: "numeric",
      month: "short",
      year: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return iso;
  }
}
