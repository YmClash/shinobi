"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Pipeline List (Phase 40-C)
// Liste des 30 derniers pipelines Jutsus d'un dépôt ⚡
// ═══════════════════════════════════════════════════════════════

import Link from "next/link";
import { usePipelines } from "@/hooks/use-pipelines";
import { TriggerButton } from "./TriggerButton";
import type { Pipeline, PipelineStatus, TriggerEvent } from "@/lib/pipeline-api";

// ── Status Config ────────────────────────────────────────────

const PIPELINE_STATUS: Record<PipelineStatus, { label: string; icon: string }> = {
  queued:    { label: "En file",   icon: "⏳" },
  running:   { label: "En cours",  icon: "🔄" },
  success:   { label: "Succès",    icon: "✅" },
  failure:   { label: "Échoué",    icon: "❌" },
  error:     { label: "Erreur",    icon: "⚠️" },
  cancelled: { label: "Annulé",    icon: "🚫" },
};

const TRIGGER_ICONS: Record<TriggerEvent, string> = {
  push: "📤",
  manual: "👆",
  mr_created: "⚔️",
  mr_merged: "🔀",
};

const TRIGGER_LABELS: Record<TriggerEvent, string> = {
  push: "push",
  manual: "manual",
  mr_created: "MR créé",
  mr_merged: "MR mergé",
};

// ── Props ────────────────────────────────────────────────────

interface PipelineListProps {
  owner: string;
  repo: string;
}

// ── Component ────────────────────────────────────────────────

export function PipelineList({ owner, repo }: PipelineListProps) {
  const { data, loading, error, refetch } = usePipelines(owner, repo);

  return (
    <div className="pl-page">
      {/* Header */}
      <div className="pl-page-header">
        <h2 className="pl-page-title">⚡ Jutsus</h2>
        <TriggerButton owner={owner} repo={repo} onTriggered={refetch} />
      </div>

      {/* Error */}
      {error && !data && (
        <div className="pl-empty">
          <div className="pl-empty-icon">⚠️</div>
          <p>{error}</p>
        </div>
      )}

      {/* Loading skeleton */}
      {loading && !data && <PipelineSkeleton />}

      {/* Empty state */}
      {data && data.pipelines.length === 0 && (
        <div className="pl-empty">
          <div className="pl-empty-icon">⚡</div>
          <p>Aucun Jutsu exécuté pour le moment.</p>
          <div className="pl-empty-hint">
            Ajoutez un fichier <code>jutsu.yml</code> à la racine de votre dépôt et poussez un commit.
          </div>
        </div>
      )}

      {/* Pipeline list */}
      {data && data.pipelines.length > 0 && (
        <div className="pl-list">
          {data.pipelines.map((pipeline) => (
            <PipelineCard
              key={pipeline.id}
              pipeline={pipeline}
              owner={owner}
              repo={repo}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Pipeline Card ────────────────────────────────────────────

function PipelineCard({
  pipeline,
  owner,
  repo,
}: {
  pipeline: Pipeline;
  owner: string;
  repo: string;
}) {
  const config = PIPELINE_STATUS[pipeline.status] ?? { label: pipeline.status, icon: "❓" };
  const triggerIcon = TRIGGER_ICONS[pipeline.trigger_event] ?? "❓";
  const triggerLabel = TRIGGER_LABELS[pipeline.trigger_event] ?? pipeline.trigger_event;
  const shortSha = pipeline.commit_id.slice(0, 7);
  const durationStr = formatDuration(pipeline.duration_ms);
  const timeStr = formatRelativeTime(pipeline.created_at);

  return (
    <Link
      href={`/${owner}/${repo}/jutsus/${pipeline.id}`}
      className="pl-card"
    >
      <div className="pl-card-left">
        {/* Status dot */}
        <span className={`pl-status-dot pl-status-dot--${pipeline.status}`} />

        {/* Info */}
        <div className="pl-card-info">
          <div className="pl-card-name">
            {config.icon} {pipeline.pipeline_name ?? "Pipeline"}
          </div>
          <div className="pl-card-meta">
            <span className="pl-commit-sha">{shortSha}</span>
            <span
              className="pl-trigger-badge"
              data-trigger={pipeline.trigger_event}
            >
              {triggerIcon} {triggerLabel}
            </span>
            <span>{config.label}</span>
          </div>
        </div>
      </div>

      {/* Right side — duration + time */}
      <div className="pl-card-right">
        {durationStr !== "—" && (
          <span className="pl-card-duration">⏱ {durationStr}</span>
        )}
        <span className="pl-card-time">{timeStr}</span>
      </div>
    </Link>
  );
}

// ── Skeleton ─────────────────────────────────────────────────

function PipelineSkeleton() {
  return (
    <div className="pl-skeleton">
      {[1, 2, 3].map((i) => (
        <div key={i} className="pl-skeleton-row" />
      ))}
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

function formatRelativeTime(iso: string): string {
  try {
    const d = new Date(iso);
    const diff = Date.now() - d.getTime();
    const s = Math.floor(diff / 1000);
    if (s < 60) return "à l'instant";
    const m = Math.floor(s / 60);
    if (m < 60) return `il y a ${m}min`;
    const h = Math.floor(m / 60);
    if (h < 24) return `il y a ${h}h`;
    const days = Math.floor(h / 24);
    return `il y a ${days}j`;
  } catch {
    return iso;
  }
}
