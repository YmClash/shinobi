"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import {
  buildRepoPrefix,
  listOperations,
  listCheckpoints,
  type Operation,
  type OperationsResponse,
  type Checkpoint,
} from "@/lib/api";

// ── Page des Commits — Style GitHub ────────────────────────────────
// Route: /[owner]/[repo]/commits

export default function CommitsPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const router = useRouter();
  const owner = params.owner;
  const repo = params.repo;
  const prefix = buildRepoPrefix(owner, repo);

  const [operations, setOperations] = useState<Operation[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const perPage = 30;

  // ── Axe 3 : Map<checkpoint_id, Checkpoint> ─────────────────────
  // Indexé par cp.id (UUID indestructible), pas par commit_id (hash volatile).
  const [checkpointMap, setCheckpointMap] = useState<Map<string, Checkpoint>>(new Map());

  useEffect(() => {
    listCheckpoints(prefix)
      .then((data) => {
        const map = new Map<string, Checkpoint>();
        for (const cp of data.checkpoints) {
          map.set(cp.id, cp); // Clé = UUID du checkpoint, pas le hash Git
        }
        setCheckpointMap(map);
      })
      .catch(() => {}); // Non-bloquant
  }, [prefix]);

  /**
   * Corrélation indestructible : extrait AI-Checkpoint: <uuid> du message de commit.
   * Survit aux rebases, cherry-picks et mutations jj car le message est préservé.
   */
  function findCheckpoint(description: string): Checkpoint | undefined {
    const match = description.match(/AI-Checkpoint:\s*([a-f0-9-]+)/i);
    if (!match) return undefined;
    return checkpointMap.get(match[1]);
  }

  useEffect(() => {
    async function load() {
      setLoading(true);
      setError(null);
      try {
        const data: OperationsResponse = await listOperations(
          prefix,
          page * perPage
        );
        setOperations(data.operations);
        setTotalCount(data.total_count ?? data.count);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load");
      } finally {
        setLoading(false);
      }
    }
    load();
  }, [prefix, page]);

  const displayedOps = operations.slice((page - 1) * perPage, page * perPage);
  const hasMore = operations.length < totalCount;

  function formatRelativeDate(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return "just now";
    if (diffMins < 60) return `${diffMins} minute${diffMins > 1 ? "s" : ""} ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours} hour${diffHours > 1 ? "s" : ""} ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 30) return `${diffDays} day${diffDays > 1 ? "s" : ""} ago`;
    return date.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  }

  function shortHash(contentId: string): string {
    return contentId.substring(0, 7);
  }

  function groupByDate(ops: Operation[]): Record<string, Operation[]> {
    const groups: Record<string, Operation[]> = {};
    for (const op of ops) {
      const date = new Date(op.created_at).toLocaleDateString("en-US", {
        weekday: "long",
        month: "long",
        day: "numeric",
        year: "numeric",
      });
      if (!groups[date]) groups[date] = [];
      groups[date].push(op);
    }
    return groups;
  }

  const grouped = groupByDate(displayedOps);

  return (
    <div className="commits-page">
      {/* Header */}
      <div className="commits-header">
        <div className="commits-header-left">
          <button
            className="commits-back-btn"
            onClick={() => router.push(`/${owner}/${repo}`)}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M7.78 12.53a.75.75 0 01-1.06 0L2.47 8.28a.75.75 0 010-1.06l4.25-4.25a.75.75 0 011.06 1.06L4.81 7h7.44a.75.75 0 010 1.5H4.81l2.97 2.97a.75.75 0 010 1.06z" />
            </svg>
            Back
          </button>
          <h1 className="commits-title">
            <svg width="20" height="20" viewBox="0 0 16 16" fill="currentColor" className="commits-icon">
              <path fillRule="evenodd" d="M10.5 7.75a2.5 2.5 0 11-5 0 2.5 2.5 0 015 0zm1.43.75a4.002 4.002 0 01-7.86 0H.75a.75.75 0 110-1.5h3.32a4.001 4.001 0 017.86 0h3.32a.75.75 0 110 1.5h-3.32z" />
            </svg>
            Commits
          </h1>
        </div>
        <div className="commits-count-badge">
          {totalCount} commit{totalCount !== 1 ? "s" : ""}
        </div>
      </div>

      {/* Loading */}
      {loading && (
        <div className="commits-loading">
          <div className="commits-spinner" />
          <span>Loading commit history...</span>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="commits-error">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zM0 8a8 8 0 1116 0A8 8 0 010 8zm9-3a1 1 0 11-2 0 1 1 0 012 0zM8 6.75a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 018 6.75z" />
          </svg>
          {error}
        </div>
      )}

      {/* Commit Groups */}
      {!loading && !error && (
        <div className="commits-timeline">
          {Object.entries(grouped).map(([date, ops]) => (
            <div key={date} className="commits-group">
              <div className="commits-date-header">
                <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                  <path fillRule="evenodd" d="M4.75 0a.75.75 0 01.75.75V2h5V.75a.75.75 0 011.5 0V2h1.25c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0113.25 16H2.75A1.75 1.75 0 011 14.25V3.75C1 2.784 1.784 2 2.75 2H4V.75A.75.75 0 014.75 0zm0 3.5h8.5a.25.25 0 01.25.25V6h-11V3.75a.25.25 0 01.25-.25h2zm-2 4.25V14.25c0 .138.112.25.25.25h10.5a.25.25 0 00.25-.25V7.75H2.75z" />
                </svg>
                <span>Commits on {date}</span>
              </div>
              <div className="commits-list">
                {ops.map((op) => {
                  const aiCtx = findCheckpoint(op.description);
                  return (
                  <div
                    key={op.id}
                    className={`commit-row ${aiCtx ? "commit-row-ai" : ""}`}
                    onClick={() =>
                      router.push(`/${owner}/${repo}/commits/${op.id}`)
                    }
                  >
                    <div className="commit-row-left">
                      {/* Avatar */}
                      <div className="commit-avatar">
                        <svg width="20" height="20" viewBox="0 0 16 16" fill="currentColor">
                          <path fillRule="evenodd" d="M10.5 5a2.5 2.5 0 11-5 0 2.5 2.5 0 015 0zm.061 3.073a4 4 0 10-5.123 0 6.004 6.004 0 00-3.431 5.142.75.75 0 001.498.07 4.5 4.5 0 018.99 0 .75.75 0 101.498-.07 6.005 6.005 0 00-3.432-5.142z" />
                        </svg>
                      </div>
                      <div className="commit-info">
                        <div className="commit-message">
                          {op.description || "No commit message"}
                          {/* Axe 1 : Badge IA */}
                          {aiCtx && (
                            <span className="commit-ai-badge" title={`AI Context: ${aiCtx.agent} · Session ${aiCtx.session_id.substring(0, 8)} · ${aiCtx.artifact_count} artifact(s)`}>
                              <span className="commit-ai-badge-icon">🧠</span>
                              <span className="commit-ai-badge-label">{aiCtx.agent}</span>
                            </span>
                          )}
                        </div>
                        <div className="commit-meta">
                          <span className="commit-author">
                            {op.author_id.substring(0, 8)}
                          </span>
                          <span className="commit-meta-sep">·</span>
                          <span className="commit-date">
                            {formatRelativeDate(op.created_at)}
                          </span>
                          {op.parent_ids.length > 0 && (
                            <>
                              <span className="commit-meta-sep">·</span>
                              <span className="commit-parents">
                                {op.parent_ids.length} parent{op.parent_ids.length > 1 ? "s" : ""}
                              </span>
                            </>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="commit-row-right">
                      <code className="commit-sha">{shortHash(op.content_id)}</code>
                      <button
                        className="commit-copy-btn"
                        title="Copy full SHA"
                        onClick={(e) => {
                          e.stopPropagation();
                          navigator.clipboard.writeText(op.content_id);
                        }}
                      >
                        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                          <path fillRule="evenodd" d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 010 1.5h-1.5a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-1.5a.75.75 0 011.5 0v1.5A1.75 1.75 0 019.25 16h-7.5A1.75 1.75 0 010 14.25v-7.5z" />
                          <path fillRule="evenodd" d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0114.25 11h-7.5A1.75 1.75 0 015 9.25v-7.5zm1.75-.25a.25.25 0 00-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 00.25-.25v-7.5a.25.25 0 00-.25-.25h-7.5z" />
                        </svg>
                      </button>
                    </div>
                  </div>
                  );
                })}
              </div>
            </div>
          ))}

          {/* Load More / No commits */}
          {displayedOps.length === 0 && !loading && (
            <div className="commits-empty">
              <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" opacity="0.3">
                <path fillRule="evenodd" d="M10.5 7.75a2.5 2.5 0 11-5 0 2.5 2.5 0 015 0zm1.43.75a4.002 4.002 0 01-7.86 0H.75a.75.75 0 110-1.5h3.32a4.001 4.001 0 017.86 0h3.32a.75.75 0 110 1.5h-3.32z" />
              </svg>
              <p>No commits yet</p>
            </div>
          )}

          {hasMore && (
            <button
              className="commits-load-more"
              onClick={() => setPage((p) => p + 1)}
              disabled={loading}
            >
              Show older commits
            </button>
          )}
        </div>
      )}
    </div>
  );
}
