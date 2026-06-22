"use client";

import { useMemo } from "react";
import {
  AreaChart,
  Area,
  Tooltip,
  ResponsiveContainer,
  ReferenceLine,
} from "recharts";
import { useScoreHistory } from "@/hooks/use-api";
import type { ScorePoint } from "@/lib/api";

// ── Types ───────────────────────────────────────────────────

interface ChartPoint {
  index: number;
  score: number;
  percent: number;
  label: string;
  operationId: string;
}

const TREND_ICONS: Record<string, string> = {
  rising: "↗",
  falling: "↘",
  stable: "→",
};

const TREND_LABELS: Record<string, string> = {
  rising: "Qualité en hausse",
  falling: "Qualité en baisse",
  stable: "Qualité stable",
};

// ── Composant principal ─────────────────────────────────────

interface ScoreSparklineProps {
  limit?: number;
}

/**
 * Électrocardiogramme du projet — mini-graphique sparkline
 * montrant la tendance des scores Oracle sur les N derniers commits.
 */
export function ScoreSparkline({ limit = 10 }: ScoreSparklineProps) {
  const { data, loading } = useScoreHistory(limit);

  const chartData = useMemo(() => {
    if (!data || data.scores.length === 0) return [];

    // Les scores arrivent DESC (récent en premier) — on inverse pour le graphique.
    const reversed = [...data.scores].reverse();

    return reversed.map((point: ScorePoint, index: number): ChartPoint => ({
      index,
      score: point.score,
      percent: Math.round(point.score * 100),
      label: new Date(point.created_at).toLocaleDateString("fr-FR", {
        day: "2-digit",
        month: "short",
        hour: "2-digit",
        minute: "2-digit",
      }),
      operationId: point.operation_id.substring(0, 8),
    }));
  }, [data]);

  // Pas de données — ne rien afficher.
  if (!data || data.scores.length === 0) {
    if (loading) return null;
    return null;
  }

  const { average, trend, count } = data;
  const trendIcon = TREND_ICONS[trend] ?? "→";
  const trendLabel = TREND_LABELS[trend] ?? "Qualité stable";
  const avgPercent = average !== null ? Math.round(average * 100) : null;

  // Couleur du gradient selon la moyenne.
  const gradientColor =
    average !== null && average >= 0.8
      ? "oklch(0.72 0.22 150)"
      : average !== null && average >= 0.5
        ? "oklch(0.75 0.18 80)"
        : "oklch(0.60 0.25 25)";

  return (
    <div className="sparkline-container" id="score-sparkline">
      {/* ── Icône ECG ──────────────── */}
      <div className="sparkline-ecg-icon">🫀</div>

      {/* ── Sparkline Chart ────────── */}
      <div className="sparkline-chart">
        <ResponsiveContainer width="100%" height={48}>
          <AreaChart data={chartData} margin={{ top: 4, right: 4, bottom: 4, left: 4 }}>
            <defs>
              <linearGradient id="scoreGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor={gradientColor} stopOpacity={0.4} />
                <stop offset="100%" stopColor={gradientColor} stopOpacity={0.05} />
              </linearGradient>
            </defs>
            {average !== null && (
              <ReferenceLine
                y={average * 100}
                stroke="var(--muted-foreground)"
                strokeDasharray="3 3"
                strokeOpacity={0.3}
              />
            )}
            <Area
              type="monotone"
              dataKey="percent"
              stroke={gradientColor}
              strokeWidth={2}
              fill="url(#scoreGradient)"
              dot={false}
              activeDot={{
                r: 4,
                stroke: gradientColor,
                strokeWidth: 2,
                fill: "var(--background)",
              }}
              animationDuration={800}
              animationEasing="ease-out"
            />
            <Tooltip content={<SparklineTooltip />} />
          </AreaChart>
        </ResponsiveContainer>
      </div>

      {/* ── Métriques ──────────────── */}
      <div className="sparkline-metrics">
        {avgPercent !== null && (
          <div className="sparkline-average">
            <span className="sparkline-average-value">{avgPercent}%</span>
            <span className="sparkline-average-label">moy.</span>
          </div>
        )}
        <div className={`sparkline-trend sparkline-trend-${trend}`}>
          <span className="sparkline-trend-icon">{trendIcon}</span>
          <span className="sparkline-trend-label">{trendLabel}</span>
        </div>
        <div className="sparkline-count">
          {count} review{count > 1 ? "s" : ""}
        </div>
      </div>
    </div>
  );
}

// ── Tooltip Custom ──────────────────────────────────────────

interface TooltipPayload {
  payload?: ChartPoint;
}

function SparklineTooltip({ active, payload }: { active?: boolean; payload?: TooltipPayload[] }) {
  if (!active || !payload || payload.length === 0 || !payload[0].payload) return null;

  const point = payload[0].payload;
  const scoreColor =
    point.score >= 0.8
      ? "oklch(0.72 0.22 150)"
      : point.score >= 0.5
        ? "oklch(0.75 0.18 80)"
        : "oklch(0.60 0.25 25)";

  return (
    <div className="sparkline-tooltip">
      <div className="sparkline-tooltip-score" style={{ color: scoreColor }}>
        {point.percent}%
      </div>
      <div className="sparkline-tooltip-meta">
        <span>op: {point.operationId}…</span>
        <span>{point.label}</span>
      </div>
    </div>
  );
}
