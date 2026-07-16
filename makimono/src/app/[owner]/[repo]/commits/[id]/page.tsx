"use client";

import { useParams, useRouter } from "next/navigation";
import { buildRepoPrefix } from "@/lib/api";
import { useOperation, useCommitDiff } from "@/hooks/use-api";
import { UnifiedDiffViewer } from "@/components/operations/unified-diff-viewer";

// ── Page de Détail d'un Commit — Diff Colorisé ────────────────────
// Route: /[owner]/[repo]/commits/[id]

export default function CommitDetailPage() {
  const params = useParams<{ owner: string; repo: string; id: string }>();
  const router = useRouter();
  const { owner, repo, id } = params;
  const prefix = buildRepoPrefix(owner, repo);

  const operation = useOperation(prefix, id);
  const diff = useCommitDiff(prefix, id);

  const op = operation.data;
  const diffData = diff.data;

  function shortHash(hash: string): string {
    return hash.substring(0, 7);
  }

  function formatDate(dateStr: string): string {
    const d = new Date(dateStr);
    return d.toLocaleDateString("en-US", {
      weekday: "long",
      month: "long",
      day: "numeric",
      year: "numeric",
    }) + " at " + d.toLocaleTimeString("en-US", {
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  return (
    <div className="commit-detail-page">
      {/* Header */}
      <div className="commit-detail-header">
        <button
          className="commits-back-btn"
          onClick={() => router.push(`/${owner}/${repo}/commits`)}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M7.78 12.53a.75.75 0 01-1.06 0L2.47 8.28a.75.75 0 010-1.06l4.25-4.25a.75.75 0 011.06 1.06L4.81 7h7.44a.75.75 0 010 1.5H4.81l2.97 2.97a.75.75 0 010 1.06z" />
          </svg>
          Back to commits
        </button>
      </div>

      {/* Loading */}
      {(operation.loading || diff.loading) && (
        <div className="commits-loading">
          <div className="commits-spinner" />
          <span>Loading commit details...</span>
        </div>
      )}

      {/* Error */}
      {(operation.error || diff.error) && (
        <div className="commits-error">
          {operation.error || diff.error}
        </div>
      )}

      {/* Commit Info */}
      {op && (
        <div className="commit-detail-info">
          <div className="commit-detail-message">
            {op.description || "No commit message"}
          </div>
          <div className="commit-detail-meta">
            <div className="commit-detail-meta-left">
              <div className="commit-avatar">
                <svg width="20" height="20" viewBox="0 0 16 16" fill="currentColor">
                  <path fillRule="evenodd" d="M10.5 5a2.5 2.5 0 11-5 0 2.5 2.5 0 015 0zm.061 3.073a4 4 0 10-5.123 0 6.004 6.004 0 00-3.431 5.142.75.75 0 001.498.07 4.5 4.5 0 018.99 0 .75.75 0 101.498-.07 6.005 6.005 0 00-3.432-5.142z" />
                </svg>
              </div>
              <span className="commit-author">{op.author_id.substring(0, 8)}</span>
              <span className="commit-meta-sep">committed on</span>
              <span className="commit-date">{formatDate(op.created_at)}</span>
            </div>
            <div className="commit-detail-meta-right">
              <div className="commit-sha-group">
                <span className="commit-sha-label">commit</span>
                <code className="commit-sha-full">{op.content_id}</code>
                <button
                  className="commit-copy-btn"
                  title="Copy full SHA"
                  onClick={() => navigator.clipboard.writeText(op.content_id)}
                >
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                    <path fillRule="evenodd" d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 010 1.5h-1.5a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-1.5a.75.75 0 011.5 0v1.5A1.75 1.75 0 019.25 16h-7.5A1.75 1.75 0 010 14.25v-7.5z" />
                    <path fillRule="evenodd" d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0114.25 11h-7.5A1.75 1.75 0 015 9.25v-7.5zm1.75-.25a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-7.5a.25.25 0 00-.25-.25h-7.5z" />
                  </svg>
                </button>
              </div>
              {op.parent_ids.length > 0 && (
                <div className="commit-parent-sha">
                  <span className="commit-sha-label">parent</span>
                  <code className="commit-sha">{shortHash(op.parent_ids[0])}</code>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Unified Diff Viewer (extracted component) */}
      {diffData && (
        <UnifiedDiffViewer
          files={diffData.files}
          stats={diffData.stats}
        />
      )}
    </div>
  );
}
