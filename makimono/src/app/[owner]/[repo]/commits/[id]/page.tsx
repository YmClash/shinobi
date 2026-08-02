"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import {
  buildRepoPrefix,
  listCheckpoints,
  type Checkpoint,
} from "@/lib/api";
import { useOperation, useCommitDiff } from "@/hooks/use-api";
import { UnifiedDiffViewer } from "@/components/operations/unified-diff-viewer";

// ── Page de Détail d'un Commit — Diff + AI Context ────────────────
// Route: /[owner]/[repo]/commits/[id]

type TabKey = "diff" | "ai-context";

export default function CommitDetailPage() {
  const params = useParams<{ owner: string; repo: string; id: string }>();
  const router = useRouter();
  const { owner, repo, id } = params;
  const prefix = buildRepoPrefix(owner, repo);

  const operation = useOperation(prefix, id);
  const diff = useCommitDiff(prefix, id);

  const op = operation.data;
  const diffData = diff.data;

  // ── AI Context ──────────────────────────────────────────────
  const [activeTab, setActiveTab] = useState<TabKey>("diff");
  const [checkpoint, setCheckpoint] = useState<Checkpoint | null>(null);
  const [hasAiContext, setHasAiContext] = useState(false);

  useEffect(() => {
    if (!op) return;
    // Corrélation indestructible : regex sur le trailer AI-Checkpoint dans le message
    const match = op.description.match(/AI-Checkpoint:\s*([a-f0-9-]+)/i);
    if (!match) return;
    const checkpointId = match[1];

    listCheckpoints(prefix)
      .then((data) => {
        const found = data.checkpoints.find((cp) => cp.id === checkpointId);
        if (found) {
          setCheckpoint(found);
          setHasAiContext(true);
        }
      })
      .catch(() => {});
  }, [op, prefix]);

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

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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
            {/* AI Badge inline */}
            {hasAiContext && checkpoint && (
              <span className="commit-ai-badge" title={`AI Context: ${checkpoint.agent}`}>
                <span className="commit-ai-badge-icon">🧠</span>
                <span className="commit-ai-badge-label">{checkpoint.agent}</span>
              </span>
            )}
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

      {/* Tab Bar — Diff / AI Context */}
      {op && (
        <div className="commit-detail-tabs">
          <button
            className={`commit-detail-tab ${activeTab === "diff" ? "commit-detail-tab-active" : ""}`}
            onClick={() => setActiveTab("diff")}
          >
            <span className="commit-detail-tab-icon">📝</span>
            Diff
            {diffData && (
              <span style={{ opacity: 0.6, fontSize: "0.65rem" }}>
                {diffData.stats.files_changed} file{diffData.stats.files_changed !== 1 ? "s" : ""}
              </span>
            )}
          </button>
          <button
            className={`commit-detail-tab ${activeTab === "ai-context" ? "commit-detail-tab-active" : ""}`}
            onClick={() => setActiveTab("ai-context")}
          >
            <span className="commit-detail-tab-icon">🧠</span>
            AI Context
            {hasAiContext && (
              <span style={{
                width: 6, height: 6, borderRadius: "50%",
                background: "var(--cp-agent-antigravity)", display: "inline-block",
                marginLeft: "0.25rem"
              }} />
            )}
          </button>
        </div>
      )}

      {/* Tab Content: Diff */}
      {activeTab === "diff" && diffData && (
        <UnifiedDiffViewer
          files={diffData.files}
          stats={diffData.stats}
        />
      )}

      {/* Tab Content: AI Context */}
      {activeTab === "ai-context" && (
        <div className="ai-context-panel">
          {checkpoint ? (
            <>
              {/* Metadata Grid */}
              <div className="ai-context-meta">
                <div className="ai-context-meta-item">
                  <span className="ai-context-meta-label">Agent</span>
                  <span className="ai-context-meta-value">
                    {checkpoint.agent === "antigravity" ? "🤖" : "🐙"} {checkpoint.agent}
                  </span>
                </div>
                <div className="ai-context-meta-item">
                  <span className="ai-context-meta-label">Session ID</span>
                  <code className="ai-context-meta-value ai-context-meta-mono">
                    {checkpoint.session_id}
                  </code>
                </div>
                <div className="ai-context-meta-item">
                  <span className="ai-context-meta-label">Checkpoint ID</span>
                  <code className="ai-context-meta-value ai-context-meta-mono">
                    {checkpoint.id}
                  </code>
                </div>
                <div className="ai-context-meta-item">
                  <span className="ai-context-meta-label">Captured</span>
                  <span className="ai-context-meta-value">
                    {new Date(checkpoint.created_at).toLocaleString()}
                  </span>
                </div>
                <div className="ai-context-meta-item">
                  <span className="ai-context-meta-label">IPFS CID</span>
                  <code className="ai-context-meta-value ai-context-meta-mono">
                    {checkpoint.ipfs_cid}
                  </code>
                </div>
                <div className="ai-context-meta-item">
                  <span className="ai-context-meta-label">Total Size</span>
                  <span className="ai-context-meta-value">
                    {formatSize(checkpoint.total_size)} · {checkpoint.artifact_count} artifact{checkpoint.artifact_count !== 1 ? "s" : ""}
                  </span>
                </div>
                {checkpoint.message && (
                  <div className="ai-context-meta-item" style={{ gridColumn: "1 / -1" }}>
                    <span className="ai-context-meta-label">Message</span>
                    <span className="ai-context-meta-value">{checkpoint.message}</span>
                  </div>
                )}
              </div>

              {/* IPFS Quick Links */}
              <div className="ai-artifacts-header">
                <span className="ai-artifacts-header-icon">📦</span>
                IPFS Content
              </div>
              <div className="ai-artifacts-list">
                <div className="ai-artifact-row">
                  <div className="ai-artifact-info">
                    <span className="ai-artifact-icon">🗂️</span>
                    <span className="ai-artifact-name">{checkpoint.ipfs_cid}</span>
                  </div>
                  <div style={{ display: "flex", gap: "0.35rem" }}>
                    <a
                      className="ai-artifact-view-btn"
                      href={`/ipfs/view/${checkpoint.ipfs_cid}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      onClick={(e) => e.stopPropagation()}
                    >
                      View
                    </a>
                    <button
                      className="ai-artifact-view-btn"
                      onClick={() => navigator.clipboard.writeText(checkpoint.ipfs_cid)}
                    >
                      Copy CID
                    </button>
                  </div>
                </div>
              </div>
            </>
          ) : (
            <div className="ai-context-empty">
              <div className="ai-context-empty-icon">🥷</div>
              <p className="ai-context-empty-text">
                No AI context associated with this commit.<br />
                Use <code>anbu checkpoint --latest</code> to capture context.
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
