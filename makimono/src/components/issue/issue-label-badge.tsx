"use client";
// Issue Label Badge — Phase 33
// Auto-calculates text contrast based on background color

import type { IssueLabel } from "@/lib/issue-api";

interface Props { label: IssueLabel; onRemove?: () => void; }

function getContrastColor(hex: string): string {
  const c = hex.replace("#", "");
  const r = parseInt(c.substring(0, 2), 16);
  const g = parseInt(c.substring(2, 4), 16);
  const b = parseInt(c.substring(4, 6), 16);
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return luminance > 0.5 ? "#1a1a2e" : "#ffffff";
}

export function IssueLabelBadge({ label, onRemove }: Props) {
  const textColor = getContrastColor(label.color);
  return (
    <span
      className="issue-label-badge"
      style={{ backgroundColor: label.color, color: textColor }}
    >
      {label.name}
      {onRemove && (
        <button className="issue-label-remove" onClick={onRemove} title="Remove label">
          ×
        </button>
      )}
    </span>
  );
}
