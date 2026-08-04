"use client";

import { useState } from "react";
import type { FileDiff, DiffHunk, DiffLine } from "@/lib/api";

// ── Props ────────────────────────────────────────────────────────────

interface DiffStats {
  files_changed: number;
  additions: number;
  deletions: number;
}

type DiffLayout = "unified" | "split";

/** Phase 30 — AI Provenance metadata for gutter colorization. */
interface AiContext {
  /** Agent name (e.g. "antigravity", "copilot"). */
  agent: string;
}

interface UnifiedDiffViewerProps {
  /** Liste des fichiers modifiés avec leurs hunks et lignes. */
  files: FileDiff[];
  /** Stats globales du diff. */
  stats: DiffStats;
  /** Classe CSS additionnelle (optionnel). */
  className?: string;
  /** Phase 30 — AI Provenance context. If set, enables gutter colorization. */
  aiContext?: AiContext | null;
}

// ── Helpers ──────────────────────────────────────────────────────────

function statusIcon(status: string): string {
  switch (status) {
    case "added": return "A";
    case "modified": return "M";
    case "deleted": return "D";
    default: return "?";
  }
}

/** Transforme les lignes d'un hunk en paires gauche/droite pour le split view. */
function buildSplitPairs(lines: DiffLine[]): { left: DiffLine | null; right: DiffLine | null }[] {
  const pairs: { left: DiffLine | null; right: DiffLine | null }[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.kind === "context") {
      pairs.push({ left: line, right: line });
      i++;
    } else if (line.kind === "remove") {
      // Collecte les removes consécutifs
      const removes: DiffLine[] = [];
      while (i < lines.length && lines[i].kind === "remove") {
        removes.push(lines[i]);
        i++;
      }
      // Collecte les adds consécutifs qui suivent
      const adds: DiffLine[] = [];
      while (i < lines.length && lines[i].kind === "add") {
        adds.push(lines[i]);
        i++;
      }
      // Zip removes et adds côte à côte
      const max = Math.max(removes.length, adds.length);
      for (let j = 0; j < max; j++) {
        pairs.push({
          left: j < removes.length ? removes[j] : null,
          right: j < adds.length ? adds[j] : null,
        });
      }
    } else if (line.kind === "add") {
      pairs.push({ left: null, right: line });
      i++;
    } else {
      i++;
    }
  }

  return pairs;
}

// ── Component ────────────────────────────────────────────────────────

/**
 * UnifiedDiffViewer V2 — Diff colorisé GitHub-style + AI Provenance Gutter.
 *
 * Supporte deux layouts :
 * - **Unified** : diff classique (add/remove intercalés)
 * - **Split** : side-by-side (ancien à gauche, nouveau à droite)
 *
 * ## Phase 30 — AI Provenance
 * Quand `aiContext` est fourni, un toggle "AI Gutter" apparaît dans la
 * toolbar. Toutes les lignes `add` reçoivent une barre colorée verticale
 * dans le gutter pour signaler leur provenance IA.
 *
 * Utilisé dans :
 * - `/[owner]/[repo]/commits/[id]` (page de détail commit)
 * - `/[owner]/[repo]/operations/[id]` (onglet "Diff Complet")
 * - `/[owner]/[repo]/mrs/[number]` (onglet "Files Changed")
 */
export function UnifiedDiffViewer({ files, stats, className = "", aiContext }: UnifiedDiffViewerProps) {
  const [layout, setLayout] = useState<DiffLayout>("unified");
  // Phase 30 — AI Gutter toggle (default ON when aiContext is present)
  const [showAiGutter, setShowAiGutter] = useState(true);

  // Is AI provenance active?
  const aiActive = !!aiContext && showAiGutter;
  const agentAttr = aiContext?.agent ?? "";

  if (files.length === 0) {
    return (
      <div className="commits-empty">
        <svg width="48" height="48" viewBox="0 0 16 16" fill="currentColor" opacity="0.3">
          <path fillRule="evenodd" d="M10.5 7.75a2.5 2.5 0 11-5 0 2.5 2.5 0 015 0zm1.43.75a4.002 4.002 0 01-7.86 0H.75a.75.75 0 110-1.5h3.32a4.001 4.001 0 017.86 0h3.32a.75.75 0 110 1.5h-3.32z" />
        </svg>
        <p>Aucun fichier modifié détecté</p>
        <p style={{ fontSize: "0.75rem", opacity: 0.6 }}>
          Ce commit est peut-être un commit de métadonnées (empty tree)
        </p>
      </div>
    );
  }

  return (
    <div className={`diff-files ${className}`}>
      {/* ── Stats Bar + Layout Toggle + AI Gutter Toggle ── */}
      <div className="diff-stats-bar">
        <span className="diff-stats-info">
          Showing <strong>{stats.files_changed}</strong> changed file{stats.files_changed !== 1 ? "s" : ""} with{" "}
          <strong className="diff-stat-add">+{stats.additions}</strong> additions and{" "}
          <strong className="diff-stat-del">-{stats.deletions}</strong> deletions
        </span>

        <div className="diff-layout-toggle">
          {/* AI Gutter Toggle — only visible when aiContext is provided */}
          {aiContext && (
            <button
              className={`diff-layout-btn diff-ai-toggle-btn ${showAiGutter ? "diff-ai-toggle-active" : ""}`}
              onClick={() => setShowAiGutter((v) => !v)}
              title={showAiGutter ? "Masquer la provenance IA" : "Afficher la provenance IA"}
            >
              <span className="diff-ai-toggle-icon">🤖</span>
              <span className="diff-layout-label">AI</span>
            </button>
          )}

          <button
            className={`diff-layout-btn ${layout === "unified" ? "diff-layout-btn-active" : ""}`}
            onClick={() => setLayout("unified")}
            title="Unified view"
          >
            {/* Unified icon — stacked lines */}
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M2 3.5h12v1H2zm0 4h12v1H2zm0 4h12v1H2z" />
            </svg>
            <span className="diff-layout-label">Unified</span>
          </button>
          <button
            className={`diff-layout-btn ${layout === "split" ? "diff-layout-btn-active" : ""}`}
            onClick={() => setLayout("split")}
            title="Split view"
          >
            {/* Split icon — two columns */}
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
              <path d="M7.25 1v14h1.5V1h-1.5zM2 3.5h4v1H2zm0 4h4v1H2zm0 4h4v1H2zm8-8h4v1h-4zm0 4h4v1h-4zm0 4h4v1h-4z" />
            </svg>
            <span className="diff-layout-label">Split</span>
          </button>
        </div>
      </div>

      {/* ── AI Context Banner (when active) ───────────── */}
      {aiActive && (
        <div className="diff-ai-banner" data-agent={agentAttr}>
          <span className="diff-ai-banner-icon">🤖</span>
          <span className="diff-ai-banner-text">
            AI Provenance active — <strong>{aiContext.agent}</strong> authored additions are highlighted
          </span>
          <span className="diff-ai-banner-legend">
            <span className="diff-ai-legend-bar" data-agent={agentAttr} />
            AI-authored lines
          </span>
        </div>
      )}

      {/* ── File Tree (quick nav) ──────────────────────── */}
      <div className="diff-file-tree">
        {files.map((file: FileDiff) => (
          <a
            key={file.path}
            href={`#diff-${encodeURIComponent(file.path)}`}
            className="diff-file-tree-item"
          >
            <span className={`diff-file-status diff-file-status-${file.status}`}>
              {statusIcon(file.status)}
            </span>
            <span className="diff-file-tree-name">{file.path}</span>
            <span className="diff-file-tree-stats">
              {file.additions > 0 && <span className="diff-stat-add">+{file.additions}</span>}
              {file.deletions > 0 && <span className="diff-stat-del">-{file.deletions}</span>}
            </span>
          </a>
        ))}
      </div>

      {/* ── File Diff Panels ───────────────────────────── */}
      {files.map((file: FileDiff) => (
        <div
          key={file.path}
          id={`diff-${encodeURIComponent(file.path)}`}
          className="diff-file-panel"
        >
          <div className="diff-file-header">
            <div className="diff-file-header-left">
              <span className={`diff-file-status diff-file-status-${file.status}`}>
                {statusIcon(file.status)}
              </span>
              <span className="diff-file-name">{file.path}</span>
              {/* AI badge per file — only for added/modified files */}
              {aiActive && file.additions > 0 && (
                <span className="diff-file-ai-badge" data-agent={agentAttr}>
                  🤖 {file.additions}
                </span>
              )}
            </div>
            <div className="diff-file-header-right">
              {file.additions > 0 && (
                <span className="diff-stat-add">+{file.additions}</span>
              )}
              {file.deletions > 0 && (
                <span className="diff-stat-del">-{file.deletions}</span>
              )}
            </div>
          </div>

          {file.too_large ? (
            <div className="diff-too-large">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" opacity="0.5">
                <path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zM0 8a8 8 0 1116 0A8 8 0 010 8zm9-3a1 1 0 11-2 0 1 1 0 012 0zM8 6.75a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 018 6.75z" />
              </svg>
              <span>
                Diff too large to display ({file.additions + file.deletions} lines changed).
              </span>
            </div>
          ) : layout === "unified" ? (
            /* ═══ UNIFIED VIEW ═══ */
            <div className="diff-file-content">
              {file.hunks.map((hunk: DiffHunk, hunkIdx: number) => (
                <div key={hunkIdx} className="diff-hunk">
                  <div className="diff-hunk-header">{hunk.header}</div>
                  {hunk.lines.map((line: DiffLine, lineIdx: number) => {
                    const isAiLine = aiActive && line.kind === "add";
                    return (
                      <div
                        key={lineIdx}
                        className={`diff-line diff-line-${line.kind}${isAiLine ? " diff-line-ai" : ""}`}
                        data-agent={isAiLine ? agentAttr : undefined}
                      >
                        <span className={`diff-line-num diff-line-num-old${isAiLine ? " diff-line-num-ai" : ""}`}>
                          {line.old_line ?? ""}
                        </span>
                        <span className={`diff-line-num diff-line-num-new${isAiLine ? " diff-line-num-ai" : ""}`}>
                          {line.new_line ?? ""}
                        </span>
                        <span className="diff-line-marker">
                          {line.kind === "add"
                            ? "+"
                            : line.kind === "remove"
                            ? "-"
                            : " "}
                        </span>
                        <span className="diff-line-content">
                          {line.content || "\u00A0"}
                        </span>
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
          ) : (
            /* ═══ SPLIT VIEW ═══ */
            <div className="diff-file-content diff-split">
              {file.hunks.map((hunk: DiffHunk, hunkIdx: number) => {
                const pairs = buildSplitPairs(hunk.lines);
                return (
                  <div key={hunkIdx} className="diff-hunk">
                    <div className="diff-hunk-header diff-split-hunk-header">
                      <span className="diff-split-hunk-label">{hunk.header}</span>
                      <span className="diff-split-hunk-label">{hunk.header}</span>
                    </div>
                    {pairs.map((pair, pairIdx) => {
                      const isRightAi = aiActive && pair.right?.kind === "add";
                      return (
                        <div key={pairIdx} className="diff-split-row">
                          {/* Left side (old / removed) */}
                          <div
                            className={`diff-split-cell ${
                              pair.left === null
                                ? "diff-split-cell-empty"
                                : pair.left.kind === "remove"
                                ? "diff-line-remove"
                                : pair.left.kind === "context"
                                ? "diff-line-context"
                                : ""
                            }`}
                          >
                            <span className="diff-line-num">
                              {pair.left?.old_line ?? ""}
                            </span>
                            <span className="diff-line-marker">
                              {pair.left?.kind === "remove" ? "-" : pair.left?.kind === "context" ? " " : ""}
                            </span>
                            <span className="diff-line-content">
                              {pair.left?.content || "\u00A0"}
                            </span>
                          </div>

                          {/* Right side (new / added) */}
                          <div
                            className={`diff-split-cell ${
                              pair.right === null
                                ? "diff-split-cell-empty"
                                : pair.right.kind === "add"
                                ? "diff-line-add"
                                : pair.right.kind === "context"
                                ? "diff-line-context"
                                : ""
                            }${isRightAi ? " diff-line-ai" : ""}`}
                            data-agent={isRightAi ? agentAttr : undefined}
                          >
                            <span className={`diff-line-num${isRightAi ? " diff-line-num-ai" : ""}`}>
                              {pair.right?.new_line ?? ""}
                            </span>
                            <span className="diff-line-marker">
                              {pair.right?.kind === "add" ? "+" : pair.right?.kind === "context" ? " " : ""}
                            </span>
                            <span className="diff-line-content">
                              {pair.right?.content || "\u00A0"}
                            </span>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
