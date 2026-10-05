"use client";

// ═══════════════════════════════════════════════════════════════
// Issues List Page — Phase 33 (Le Parchemin des Doléances)
// /[owner]/[repo]/issues
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";
import { useIssues } from "@/hooks/use-issues";
import { IssueStatusBadge } from "@/components/issue/issue-status-badge";
import type { IssueStatus } from "@/lib/issue-api";

type Tab = "open" | "closed" | "all";

export default function IssuesPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const router = useRouter();
  const [tab, setTab] = useState<Tab>("open");

  const statusFilter: IssueStatus | undefined =
    tab === "all" ? undefined : tab;

  const { data, loading, error } = useIssues(
    params.owner, params.repo, statusFilter, 50, 0
  );

  return (
    <div className="issue-page">
      {/* ── Header ── */}
      <div className="issue-page-header">
        <h1 className="issue-page-title">🎯 Issues</h1>
        <Link
          href={`/${params.owner}/${params.repo}/issues/new`}
          className="issue-new-btn"
        >
          ✨ New Issue
        </Link>
      </div>

      {/* ── Tabs ── */}
      <div className="issue-tabs">
        {(["open", "closed", "all"] as Tab[]).map((t) => (
          <button
            key={t}
            className={`issue-tab ${tab === t ? "issue-tab-active" : ""}`}
            onClick={() => setTab(t)}
          >
            {t === "open" && "🟢 Open"}
            {t === "closed" && "🔴 Closed"}
            {t === "all" && "📋 All"}
            {data && t === tab && (
              <span className="issue-tab-count">{data.total}</span>
            )}
          </button>
        ))}
      </div>

      {/* ── Content ── */}
      {loading && !data && (
        <div className="issue-skeleton">
          {[...Array(5)].map((_, i) => (
            <div key={i} className="issue-skeleton-row" />
          ))}
        </div>
      )}

      {error && (
        <div className="issue-error">
          <span>⚠️</span> {error}
        </div>
      )}

      {data && data.items.length === 0 && (
        <div className="issue-empty">
          <span className="issue-empty-icon">🎯</span>
          <p>No {tab !== "all" ? tab : ""} issues yet</p>
          <Link
            href={`/${params.owner}/${params.repo}/issues/new`}
            className="issue-new-btn"
          >
            Create the first one
          </Link>
        </div>
      )}

      {data && data.items.length > 0 && (
        <div className="issue-list">
          {data.items.map((issue) => (
            <Link
              key={issue.id}
              href={`/${params.owner}/${params.repo}/issues/${issue.number}`}
              className="issue-card"
            >
              <div className="issue-card-left">
                <IssueStatusBadge status={issue.status} size="sm" />
                <div className="issue-card-info">
                  <span className="issue-card-title">{issue.title}</span>
                  <span className="issue-card-meta">
                    #{issue.number} · opened{" "}
                    {new Date(issue.created_at).toLocaleDateString("fr-FR", {
                      day: "numeric",
                      month: "short",
                    })}
                  </span>
                </div>
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
