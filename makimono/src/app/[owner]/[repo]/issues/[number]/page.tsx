"use client";

// ═══════════════════════════════════════════════════════════════
// Issue Detail Page — Phase 33
// /[owner]/[repo]/issues/[number]
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import { useParams } from "next/navigation";
import { useIssueDetail, emitIssueChanged } from "@/hooks/use-issues";
import { IssueStatusBadge } from "@/components/issue/issue-status-badge";
import { IssueLabelBadge } from "@/components/issue/issue-label-badge";
import { IssueTimeline } from "@/components/issue/issue-timeline";
import { closeIssue, reopenIssue, commentIssue } from "@/lib/issue-api";

export default function IssueDetailPage() {
  const params = useParams<{ owner: string; repo: string; number: string }>();
  const issueNumber = parseInt(params.number, 10);

  const { data, loading, error, refetch } = useIssueDetail(
    params.owner, params.repo, issueNumber
  );

  const [commentBody, setCommentBody] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  async function handleComment(e: React.FormEvent) {
    e.preventDefault();
    if (!commentBody.trim()) return;
    setSubmitting(true);
    setActionError(null);
    try {
      await commentIssue(params.owner, params.repo, issueNumber, {
        body: commentBody.trim(),
      });
      setCommentBody("");
      emitIssueChanged(params.owner, params.repo);
      refetch();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Error");
    } finally {
      setSubmitting(false);
    }
  }

  async function handleToggleStatus() {
    setSubmitting(true);
    setActionError(null);
    try {
      if (data?.status === "open") {
        await closeIssue(params.owner, params.repo, issueNumber);
      } else {
        await reopenIssue(params.owner, params.repo, issueNumber);
      }
      emitIssueChanged(params.owner, params.repo);
      refetch();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Error");
    } finally {
      setSubmitting(false);
    }
  }

  if (loading && !data) {
    return (
      <div className="issue-detail-skeleton">
        <div className="issue-skeleton-row" style={{ width: "60%" }} />
        <div className="issue-skeleton-row" style={{ width: "40%" }} />
        <div className="issue-skeleton-row" style={{ width: "80%" }} />
      </div>
    );
  }

  if (error) {
    return <div className="issue-error"><span>⚠️</span> {error}</div>;
  }

  if (!data) return null;

  return (
    <div className="issue-detail-page">
      {/* ── Header ── */}
      <div className="issue-detail-header">
        <div className="issue-detail-title-row">
          <h1 className="issue-detail-title">{data.title}</h1>
          <span className="issue-detail-number">#{data.number}</span>
        </div>
        <div className="issue-detail-meta">
          <IssueStatusBadge status={data.status} />
          <span className="issue-detail-meta-text">
            opened{" "}
            {new Date(data.created_at).toLocaleDateString("fr-FR", {
              day: "numeric", month: "long", year: "numeric",
            })}
          </span>
        </div>
      </div>

      <div className="issue-detail-layout">
        {/* ── Main content ── */}
        <div className="issue-detail-main">
          {/* Body */}
          {data.body && (
            <div className="issue-detail-body">
              <p>{data.body}</p>
            </div>
          )}

          {/* Labels */}
          {data.labels.length > 0 && (
            <div className="issue-detail-labels">
              {data.labels.map((label) => (
                <IssueLabelBadge key={label.id} label={label} />
              ))}
            </div>
          )}

          {/* Timeline */}
          <div className="issue-detail-timeline">
            <h2 className="issue-section-title">Activity</h2>
            <IssueTimeline
              comments={data.comments}
              events={data.events}
            />
          </div>

          {/* Comment form */}
          <form onSubmit={handleComment} className="issue-comment-form">
            <textarea
              className="issue-comment-textarea"
              placeholder="Leave a comment..."
              value={commentBody}
              onChange={(e) => setCommentBody(e.target.value)}
              rows={4}
            />
            {actionError && (
              <div className="issue-form-error">⚠️ {actionError}</div>
            )}
            <div className="issue-comment-form-actions">
              <button
                type="button"
                className={`issue-toggle-btn ${
                  data.status === "open" ? "issue-close-btn" : "issue-reopen-btn"
                }`}
                onClick={handleToggleStatus}
                disabled={submitting}
              >
                {data.status === "open" ? "🔒 Close Issue" : "🔓 Reopen Issue"}
              </button>
              <button
                type="submit"
                className="issue-form-submit"
                disabled={submitting || !commentBody.trim()}
              >
                {submitting ? "..." : "💬 Comment"}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
