"use client";
// Issue Status Badge — Phase 33
import type { IssueStatus } from "@/lib/issue-api";

interface Props { status: IssueStatus; size?: "sm" | "md"; }

export function IssueStatusBadge({ status, size = "md" }: Props) {
  const cls = `issue-status-badge issue-status-${status} issue-badge-${size}`;
  return (
    <span className={cls}>
      <span className="issue-badge-dot" />
      {status === "open" ? "Open" : "Closed"}
    </span>
  );
}
