"use client";

// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo]/mrs/[number] — MR Detail (Phase 26B)
// React 19 Optimistic UI for instant merge/close/review
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import {
  useMergeRequestDetail,
  useMergeRequestDiff,
  useOptimisticMrActions,
} from "@/hooks/use-mr";
import { useAuth } from "@/hooks/use-auth";
import { MrStatusBadge } from "@/components/mr/mr-status-badge";
import { MrTimeline } from "@/components/mr/mr-timeline";
import { MrReviewCard } from "@/components/mr/mr-review-card";
import { MrActionsBar } from "@/components/mr/mr-actions-bar";
import { UnifiedDiffViewer } from "@/components/operations/unified-diff-viewer";
import { MentionRenderer } from "@/components/ui/mention-renderer";

type DetailTab = "conversation" | "diff";

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

export default function MrDetailPage() {
  const params = useParams<{ owner: string; repo: string; number: string }>();
  const { user } = useAuth();
  const mrNumber = parseInt(params.number, 10);
  const [activeTab, setActiveTab] = useState<DetailTab>("conversation");

  // Data fetching
  const {
    data: detail,
    loading,
    error,
    refetch,
  } = useMergeRequestDetail(params.owner, params.repo, mrNumber);

  const { data: diffData, loading: diffLoading } = useMergeRequestDiff(
    params.owner,
    params.repo,
    mrNumber,
  );

  // Optimistic actions
  const mrActions = useOptimisticMrActions(
    params.owner,
    params.repo,
    detail,
    refetch,
  );

  const mr = mrActions.optimisticDetail;
  const isOptimistic = detail ? mr.status !== detail.status : false;

  if (loading && !detail) {
    return (
      <div className="mr-page-container">
        <div className="mr-detail-skeleton">
          <div className="mr-skeleton mr-skeleton-lg animate-shimmer" />
          <div className="mr-skeleton mr-skeleton-md animate-shimmer" />
          <div className="mr-skeleton animate-shimmer" />
          <div className="mr-skeleton animate-shimmer" />
        </div>
      </div>
    );
  }

  if (error || !detail) {
    return (
      <div className="mr-page-container">
        <div className="mr-error">
          <span className="mr-error-icon">⚠️</span>
          <span>{error ?? "Merge Request not found"}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="mr-page-container">
      {/* ── Header ─────────────────────────────────── */}
      <div className="mr-detail-header animate-fade-in-up">
        <div className="mr-detail-breadcrumb">
          <Link
            href={`/${params.owner}/${params.repo}/mrs`}
            className="mr-breadcrumb-link"
          >
            ← Back to MRs
          </Link>
        </div>

        <div className="mr-detail-title-row">
          <h1 className="mr-detail-title">{mr.title}</h1>
          <span className="mr-detail-number">#{mr.number}</span>
        </div>

        <div className="mr-detail-meta">
          <MrStatusBadge status={mr.status} optimistic={isOptimistic} />
          <span className="mr-branch-arrow mr-branch-arrow-lg">
            <span className="mr-branch-name">{mr.source_branch}</span>
            <span className="mr-branch-separator">→</span>
            <span className="mr-branch-name">{mr.target_branch}</span>
          </span>
          <span className="mr-item-dot">·</span>
          <span className="mr-detail-time">
            opened {formatRelativeTime(mr.created_at)}
          </span>

          {mr.has_conflicts && mr.status === "open" && (
            <span className="mr-conflict-badge">
              ⚠️ Conflicts
            </span>
          )}
        </div>

        {mr.description && (
          <div className="mr-detail-description"><MentionRenderer text={mr.description} /></div>
        )}
      </div>

      {/* ── Actions Bar (authenticated only) ────────── */}
      {user && (
        <div className="animate-fade-in-up stagger-1" style={{ position: 'relative', zIndex: 20 }}>
          <MrActionsBar
            status={mr.status}
            hasConflicts={mr.has_conflicts}
            actions={mrActions}
          />
        </div>
      )}

      {/* ── Tabs ───────────────────────────────────── */}
      <div className="mr-detail-tabs animate-fade-in-up stagger-2">
        <button
          className={`mr-tab ${activeTab === "conversation" ? "mr-tab-active" : ""}`}
          onClick={() => setActiveTab("conversation")}
        >
          💬 Conversation
          {mr.reviews.length > 0 && (
            <span className="mr-tab-count">{mr.reviews.length}</span>
          )}
        </button>
        <button
          className={`mr-tab ${activeTab === "diff" ? "mr-tab-active" : ""}`}
          onClick={() => setActiveTab("diff")}
        >
          📄 Files Changed
          {diffData && (
            <span className="mr-tab-count">{diffData.total_files}</span>
          )}
        </button>
      </div>

      {/* ── Conversation Tab ───────────────────────── */}
      {activeTab === "conversation" && (
        <div className="mr-conversation animate-fade-in-up stagger-3">
          {/* Reviews */}
          {mr.reviews.length > 0 && (
            <div className="mr-reviews-section">
              <h3 className="mr-section-title">Reviews</h3>
              <div className="mr-reviews-list">
                {mr.reviews.map((review) => (
                  <MrReviewCard key={review.id} review={review} />
                ))}
              </div>
            </div>
          )}

          {/* Timeline */}
          {mr.events.length > 0 && (
            <div className="mr-timeline-section">
              <h3 className="mr-section-title">Activity</h3>
              <MrTimeline events={mr.events} />
            </div>
          )}

          {mr.reviews.length === 0 && mr.events.length === 0 && (
            <div className="mr-empty-conversation">
              <span className="mr-empty-icon">💬</span>
              <p>No activity yet. Be the first to review this merge request.</p>
            </div>
          )}
        </div>
      )}

      {/* ── Diff Tab ───────────────────────────────── */}
      {activeTab === "diff" && (
        <div className="mr-diff-container animate-fade-in-up stagger-3">
          {diffLoading && !diffData && (
            <div className="mr-loading">
              <div className="mr-skeleton animate-shimmer" />
              <div className="mr-skeleton animate-shimmer" />
            </div>
          )}

          {diffData && diffData.files.length === 0 && (
            <div className="mr-empty-conversation">
              <span className="mr-empty-icon">📄</span>
              <p>No files changed in this merge request.</p>
            </div>
          )}

          {diffData && diffData.files.length > 0 && (
            <UnifiedDiffViewer
              files={diffData.files}
              stats={{
                files_changed: diffData.total_files,
                additions: diffData.files.reduce((s, f) => s + f.additions, 0),
                deletions: diffData.files.reduce((s, f) => s + f.deletions, 0),
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}
