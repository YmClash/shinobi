"use client";

// ═══════════════════════════════════════════════════════════════
// MR Timeline — Chronological event display (Phase 26B)
// ═══════════════════════════════════════════════════════════════

import type { MrEvent } from "@/lib/mr-api";

interface MrTimelineProps {
  events: MrEvent[];
}

const eventConfig: Record<
  string,
  { icon: string; label: string; color: string }
> = {
  opened: { icon: "🔓", label: "Opened", color: "mr-event-opened" },
  closed: { icon: "🚫", label: "Closed", color: "mr-event-closed" },
  merged: { icon: "🔀", label: "Merged", color: "mr-event-merged" },
  approved: { icon: "✅", label: "Approved", color: "mr-event-approved" },
  changes_requested: {
    icon: "⚡",
    label: "Changes Requested",
    color: "mr-event-changes",
  },
  reopened: { icon: "🔄", label: "Reopened", color: "mr-event-opened" },
  mentioned: { icon: "📣", label: "Mentioned", color: "mr-event-mentioned" },
};

function formatRelativeTime(dateStr: string): string {
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diffMs = now - then;
  const diffMins = Math.floor(diffMs / 60_000);
  if (diffMins < 1) return "just now";
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 30) return `${diffDays}d ago`;
  return new Date(dateStr).toLocaleDateString();
}

export function MrTimeline({ events }: MrTimelineProps) {
  if (!events.length) return null;

  return (
    <div className="mr-timeline">
      {events.map((event, idx) => {
        const config = eventConfig[event.event_type] ?? {
          icon: "📌",
          label: event.event_type,
          color: "",
        };

        // Extract useful info from payload
        const payloadInfo = event.payload
          ? Object.entries(event.payload)
              .filter(([k]) => k !== "mr_id")
              .map(([k, v]) => `${k}: ${v}`)
              .join(" · ")
          : null;

        return (
          <div
            key={event.id}
            className={`mr-timeline-item animate-fade-in-up stagger-${Math.min(idx + 1, 5)}`}
          >
            <div className="mr-timeline-dot-container">
              <div className={`mr-timeline-dot ${config.color}`}>
                {config.icon}
              </div>
              {idx < events.length - 1 && <div className="mr-timeline-line" />}
            </div>
            <div className="mr-timeline-content">
              <div className="mr-timeline-header">
                <span className="mr-timeline-label">{config.label}</span>
                <span className="mr-timeline-time">
                  {formatRelativeTime(event.created_at)}
                </span>
              </div>
              {payloadInfo && (
                <div className="mr-timeline-payload">{payloadInfo}</div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
