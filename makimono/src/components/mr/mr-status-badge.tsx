"use client";

// ═══════════════════════════════════════════════════════════════
// MR Status Badge — Animated status indicator (Phase 26B)
// ═══════════════════════════════════════════════════════════════

import type { MrStatus } from "@/lib/mr-api";

interface MrStatusBadgeProps {
  status: MrStatus;
  /** Whether the status may be optimistic (in-flight mutation). */
  optimistic?: boolean;
  className?: string;
}

const statusConfig: Record<
  MrStatus,
  { label: string; icon: string; className: string }
> = {
  open: {
    label: "Open",
    icon: "🔓",
    className: "mr-status-open",
  },
  merged: {
    label: "Merged",
    icon: "🔀",
    className: "mr-status-merged",
  },
  closed: {
    label: "Closed",
    icon: "🚫",
    className: "mr-status-closed",
  },
};

export function MrStatusBadge({
  status,
  optimistic = false,
  className = "",
}: MrStatusBadgeProps) {
  const config = statusConfig[status];
  return (
    <span
      className={`mr-status-badge ${config.className} ${
        optimistic ? "mr-status-optimistic" : ""
      } ${className}`}
    >
      <span className="mr-status-icon">{config.icon}</span>
      <span className="mr-status-label">{config.label}</span>
    </span>
  );
}
