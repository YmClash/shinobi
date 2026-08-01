"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import {
  buildRepoPrefix,
  listCheckpoints,
  type Checkpoint,
  type CheckpointsResponse,
} from "@/lib/api";

// ── ANBU AI Checkpoints — Phase 28C ────────────────────────────
// Route: /[owner]/[repo]/checkpoints

const AGENT_CONFIG: Record<string, { icon: string; color: string; label: string }> = {
  antigravity: { icon: "🤖", color: "var(--cp-agent-antigravity)", label: "Antigravity" },
  copilot:     { icon: "🐙", color: "var(--cp-agent-copilot)",     label: "Copilot" },
};

function getAgentInfo(agent: string) {
  return AGENT_CONFIG[agent.toLowerCase()] ?? { icon: "🧠", color: "var(--cp-agent-default)", label: agent };
}

export default function CheckpointsPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const owner = params.owner;
  const repo = params.repo;
  const prefix = buildRepoPrefix(owner, repo);

  const [checkpoints, setCheckpoints] = useState<Checkpoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      setLoading(true);
      setError(null);
      try {
        const data: CheckpointsResponse = await listCheckpoints(prefix);
        setCheckpoints(data.checkpoints);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load checkpoints");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, [prefix]);

  function formatRelativeDate(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return "just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 30) return `${diffDays}d ago`;
    return date.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function shortId(id: string): string {
    return id.substring(0, 8);
  }

  function shortCid(cid: string): string {
    if (cid.length <= 16) return cid;
    return `${cid.substring(0, 8)}...${cid.substring(cid.length - 6)}`;
  }

  return (
    <div className="cp-page">
      {/* Header */}
      <div className="cp-header">
        <div className="cp-header-left">
          <h1 className="cp-title">
            <span className="cp-title-icon">🧠</span>
            AI Checkpoints
          </h1>
          <span className="cp-subtitle">
            Context snapshots captured by ANBU
          </span>
        </div>
        {checkpoints.length > 0 && (
          <div className="cp-count-badge">
            {checkpoints.length} checkpoint{checkpoints.length !== 1 ? "s" : ""}
          </div>
        )}
      </div>

      {/* Loading */}
      {loading && (
        <div className="cp-loading">
          <div className="cp-spinner" />
          <span>Loading AI checkpoints...</span>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="cp-error">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zM0 8a8 8 0 1116 0A8 8 0 010 8zm9-3a1 1 0 11-2 0 1 1 0 012 0zM8 6.75a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 018 6.75z" />
          </svg>
          {error}
        </div>
      )}

      {/* Checkpoint List */}
      {!loading && !error && checkpoints.length > 0 && (
        <div className="cp-list">
          {checkpoints.map((cp) => {
            const agent = getAgentInfo(cp.agent);
            const isExpanded = expandedId === cp.id;

            return (
              <div
                key={cp.id}
                className={`cp-card ${isExpanded ? "cp-card-expanded" : ""}`}
                onClick={() => setExpandedId(isExpanded ? null : cp.id)}
              >
                {/* Card Header */}
                <div className="cp-card-header">
                  <div className="cp-card-left">
                    {/* Agent Badge */}
                    <span
                      className="cp-agent-badge"
                      style={{ "--agent-color": agent.color } as React.CSSProperties}
                    >
                      <span className="cp-agent-icon">{agent.icon}</span>
                      {agent.label}
                    </span>

                    {/* Message or Session */}
                    <div className="cp-card-info">
                      <span className="cp-card-message">
                        {cp.message || `Session ${shortId(cp.session_id)}`}
                      </span>
                      <div className="cp-card-meta">
                        <code className="cp-session-id">{shortId(cp.session_id)}</code>
                        <span className="cp-meta-sep">·</span>
                        <span className="cp-card-date">{formatRelativeDate(cp.created_at)}</span>
                        <span className="cp-meta-sep">·</span>
                        <span className="cp-artifact-count">
                          {cp.artifact_count} artifact{cp.artifact_count !== 1 ? "s" : ""}
                        </span>
                        <span className="cp-meta-sep">·</span>
                        <span className="cp-size">{formatSize(cp.total_size)}</span>
                      </div>
                    </div>
                  </div>

                  <div className="cp-card-right">
                    {/* IPFS CID */}
                    <div className="cp-ipfs-badge" title={cp.ipfs_cid}>
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" opacity="0.6">
                        <path d="M12 0L1.6 6v12L12 24l10.4-6V6L12 0zm0 2.3l8 4.6v9.2l-8 4.6-8-4.6V6.9l8-4.6z"/>
                      </svg>
                      <code>{shortCid(cp.ipfs_cid)}</code>
                    </div>

                    {/* Expand indicator */}
                    <svg
                      className={`cp-expand-icon ${isExpanded ? "cp-expand-open" : ""}`}
                      width="16" height="16" viewBox="0 0 16 16" fill="currentColor"
                    >
                      <path d="M4.427 7.427l3.396 3.396a.25.25 0 00.354 0l3.396-3.396A.25.25 0 0011.396 7H4.604a.25.25 0 00-.177.427z" />
                    </svg>
                  </div>
                </div>

                {/* Expanded Detail */}
                {isExpanded && (
                  <div className="cp-detail" onClick={(e) => e.stopPropagation()}>
                    <div className="cp-detail-grid">
                      <div className="cp-detail-item">
                        <span className="cp-detail-label">Checkpoint ID</span>
                        <code className="cp-detail-value cp-detail-mono">{cp.id}</code>
                      </div>
                      <div className="cp-detail-item">
                        <span className="cp-detail-label">Session ID</span>
                        <code className="cp-detail-value cp-detail-mono">{cp.session_id}</code>
                      </div>
                      <div className="cp-detail-item">
                        <span className="cp-detail-label">IPFS CID</span>
                        <div className="cp-detail-cid-row">
                          <code className="cp-detail-value cp-detail-mono">{cp.ipfs_cid}</code>
                          <button
                            className="cp-copy-btn"
                            title="Copy CID"
                            onClick={() => navigator.clipboard.writeText(cp.ipfs_cid)}
                          >
                            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                              <path fillRule="evenodd" d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 010 1.5h-1.5a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-1.5a.75.75 0 011.5 0v1.5A1.75 1.75 0 019.25 16h-7.5A1.75 1.75 0 010 14.25v-7.5z" />
                              <path fillRule="evenodd" d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0114.25 11h-7.5A1.75 1.75 0 015 9.25v-7.5zm1.75-.25a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-7.5a.25.25 0 00-.25-.25h-7.5z" />
                            </svg>
                          </button>
                        </div>
                      </div>
                      {cp.commit_id && (
                        <div className="cp-detail-item">
                          <span className="cp-detail-label">Commit</span>
                          <code className="cp-detail-value cp-detail-mono">{cp.commit_id}</code>
                        </div>
                      )}
                      <div className="cp-detail-item">
                        <span className="cp-detail-label">Agent</span>
                        <span className="cp-detail-value">{agent.icon} {agent.label}</span>
                      </div>
                      <div className="cp-detail-item">
                        <span className="cp-detail-label">Created</span>
                        <span className="cp-detail-value">
                          {new Date(cp.created_at).toLocaleString()}
                        </span>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Empty State */}
      {!loading && !error && checkpoints.length === 0 && (
        <div className="cp-empty">
          <div className="cp-empty-icon">🥷</div>
          <h2 className="cp-empty-title">No AI Checkpoints</h2>
          <p className="cp-empty-text">
            Use the ANBU CLI to capture and sync AI context snapshots.
          </p>
          <div className="cp-empty-code">
            <code>anbu checkpoint --latest -m &quot;My checkpoint&quot;</code>
            <br />
            <code>anbu sync --owner {owner} --repo {repo}</code>
          </div>
        </div>
      )}
    </div>
  );
}
