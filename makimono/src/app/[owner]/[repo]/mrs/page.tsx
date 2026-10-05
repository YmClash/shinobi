"use client";

// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo]/mrs — Merge Requests List (Phase 26B)
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useMergeRequests } from "@/hooks/use-mr";
import { useAuth } from "@/hooks/use-auth";
import { MrStatusBadge } from "@/components/mr/mr-status-badge";
import type { MrStatus } from "@/lib/mr-api";

type TabStatus = MrStatus | "all";

const tabs: { value: TabStatus; label: string; icon: string }[] = [
  { value: "open", label: "Open", icon: "🔓" },
  { value: "merged", label: "Merged", icon: "🔀" },
  { value: "closed", label: "Closed", icon: "🚫" },
  { value: "all", label: "All", icon: "📋" },
];

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

export default function MrListPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const { user } = useAuth();
  const [activeTab, setActiveTab] = useState<TabStatus>("open");

  const statusFilter = activeTab === "all" ? undefined : activeTab;
  const { data, loading, error } = useMergeRequests(
    params.owner,
    params.repo,
    statusFilter,
  );

  return (
    <div className="mr-page-container">
      {/* ── Header ─────────────────────────────────── */}
      <div className="mr-page-header">
        <div className="mr-page-title-row">
          <h1 className="mr-page-title">
            <span className="mr-page-icon">⚔️</span>
            Merge Requests
          </h1>
          {user && (
            <Link
              href={`/${params.owner}/${params.repo}/mrs/new`}
              className="mr-new-btn"
            >
              <span>+</span> New MR
            </Link>
          )}
        </div>

        {/* ── Status Tabs ──────────────────────────── */}
        <div className="mr-tabs">
          {tabs.map((tab) => (
            <button
              key={tab.value}
              className={`mr-tab ${activeTab === tab.value ? "mr-tab-active" : ""}`}
              onClick={() => setActiveTab(tab.value)}
            >
              <span className="mr-tab-icon">{tab.icon}</span>
              <span className="mr-tab-label">{tab.label}</span>
              {data && tab.value !== "all" && (
                <span className="mr-tab-count">
                  {data.items.filter(
                    (mr) => tab.value === "all" || mr.status === tab.value,
                  ).length}
                </span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* ── Content ────────────────────────────────── */}
      <div className="mr-list-content">
        {loading && !data && (
          <div className="mr-loading">
            {[1, 2, 3].map((i) => (
              <div key={i} className="mr-skeleton animate-shimmer" />
            ))}
          </div>
        )}

        {error && (
          <div className="mr-error">
            <span className="mr-error-icon">⚠️</span>
            <span>{error}</span>
          </div>
        )}

        {data && data.items.length === 0 && (
          <div className="mr-empty">
            <div className="mr-empty-icon">⚔️</div>
            <h3 className="mr-empty-title">No merge requests</h3>
            <p className="mr-empty-desc">
              {activeTab === "open"
                ? "There are no open merge requests for this repository."
                : `No ${activeTab} merge requests found.`}
            </p>
            {user && activeTab === "open" && (
              <Link
                href={`/${params.owner}/${params.repo}/mrs/new`}
                className="mr-new-btn mr-new-btn-empty"
              >
                Create the first MR
              </Link>
            )}
          </div>
        )}

        {data &&
          data.items.map((mr, idx) => (
            <Link
              key={mr.id}
              href={`/${params.owner}/${params.repo}/mrs/${mr.number}`}
              className={`mr-list-item animate-fade-in-up stagger-${Math.min(idx + 1, 5)}`}
            >
              <div className="mr-item-main">
                <div className="mr-item-title-row">
                  <MrStatusBadge status={mr.status} />
                  <span className="mr-item-title">{mr.title}</span>
                  <span className="mr-item-number">#{mr.number}</span>
                </div>
                <div className="mr-item-meta">
                  <span className="mr-branch-arrow">
                    <span className="mr-branch-name">{mr.source_branch}</span>
                    <span className="mr-branch-separator">→</span>
                    <span className="mr-branch-name">{mr.target_branch}</span>
                  </span>
                  <span className="mr-item-dot">·</span>
                  <span className="mr-item-time">
                    {formatRelativeTime(mr.created_at)}
                  </span>
                </div>
              </div>
            </Link>
          ))}
      </div>

      {/* ── Pagination hint ────────────────────────── */}
      {data && data.total > 30 && (
        <div className="mr-pagination-hint">
          Showing {data.items.length} of {data.total} merge requests
        </div>
      )}
    </div>
  );
}
