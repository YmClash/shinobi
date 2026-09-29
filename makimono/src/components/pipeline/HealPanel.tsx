"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — HealPanel (Phase 41-B · Kage Bunshin UI) 🥷⚡
// Panneau latéral détaillé pour les tentatives d'auto-healing.
//
// Vegapunk Tweaks intégrés :
// VP-12 — renderHunkDiff() : split('\n') + lignes colorées diff
// VP-14 — Autopsie des échecs : messages confiance faible
// VP-17 — Rigidité Monospace : pre-wrap + font-mono pour les hunks
// VP-18 — Click Outside + Escape pour fermer le drawer
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import type { HealAttempt, PatchHunk } from "@/lib/pipeline-api";

// ── Props ────────────────────────────────────────────────────

interface HealPanelProps {
  /** Heal attempt à afficher. */
  heal: HealAttempt | null;
  /** Contrôle l'ouverture du panneau. */
  open: boolean;
  /** Callback de fermeture. */
  onClose: () => void;
  /** Owner du repo (pour le lien MR). */
  owner: string;
  /** Repo name (pour le lien MR). */
  repo: string;
}

// ── Status Config ────────────────────────────────────────────

const HEAL_STATUS_CONFIG: Record<string, { label: string; icon: string }> = {
  pending: { label: "Sensei analyse…", icon: "🧠" },
  healing: { label: "Healing…", icon: "⚔️" },
  success: { label: "Healed", icon: "✅" },
  failed: { label: "Échoué", icon: "❌" },
};

// ── Component ────────────────────────────────────────────────

export function HealPanel({
  heal,
  open,
  onClose,
  owner,
  repo,
}: HealPanelProps) {
  const [showLogs, setShowLogs] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  // VP-18: Close on Escape
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape" && open) onClose();
    },
    [open, onClose],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  // VP-18: Close on click outside
  const handleOverlayClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose],
  );

  // Reset logs expansion when heal changes
  useEffect(() => {
    setShowLogs(false);
  }, [heal?.id]);

  if (!open || !heal) return null;

  const statusConfig = HEAL_STATUS_CONFIG[heal.status] ?? { label: heal.status, icon: "❓" };
  const confidencePct = heal.confidence !== null ? Math.round(heal.confidence * 100) : null;
  const confidenceLevel =
    confidencePct !== null
      ? confidencePct < 40
        ? "low"
        : confidencePct < 70
          ? "medium"
          : "high"
      : null;

  return (
    <>
      {/* Overlay (VP-18: click outside to close) */}
      <div className="pl-heal-overlay" onClick={handleOverlayClick} />

      {/* Panel */}
      <div className="pl-heal-panel" ref={panelRef}>
        {/* Header */}
        <div className="pl-heal-header">
          <div className="pl-heal-title-group">
            <h3 className="pl-heal-title">
              ⚔️ Auto-Heal — {heal.stage_name}
            </h3>
            <span className={`pl-heal-status-badge pl-heal-status-badge--${heal.status}`}>
              {statusConfig.icon} {statusConfig.label}
            </span>
          </div>
          <button
            className="pl-close-btn"
            onClick={onClose}
            title="Fermer (Esc)"
          >
            ✕
          </button>
        </div>

        {/* Content */}
        <div className="pl-heal-content">
          {/* 🧠 Diagnostic */}
          <div className="pl-heal-section">
            <div className="pl-heal-section-title">🧠 Diagnostic Sensei</div>
            <div className="pl-heal-diagnosis">{heal.diagnosis}</div>
          </div>

          {/* 📝 Fix proposé */}
          {heal.patch_summary && (
            <div className="pl-heal-section">
              <div className="pl-heal-section-title">📝 Fix proposé</div>
              <div className="pl-heal-diagnosis">{heal.patch_summary}</div>
            </div>
          )}

          {/* 🔧 Hunks diff (VP-12) */}
          {heal.hunks.length > 0 && (
            <div className="pl-heal-section">
              <div className="pl-heal-section-title">
                🔧 Hunks ({heal.hunks.length})
              </div>
              <div className="pl-heal-hunks">
                {heal.hunks.map((hunk, i) => (
                  <HunkDiff key={i} hunk={hunk} />
                ))}
              </div>
            </div>
          )}

          {/* 📊 Confiance + LLM Metadata */}
          {(confidencePct !== null || heal.llm_model) && (
            <div className="pl-heal-section">
              <div className="pl-heal-section-title">📊 Métriques LLM</div>

              {/* Confidence gauge */}
              {confidencePct !== null && confidenceLevel && (
                <div className="pl-heal-confidence-row">
                  <span>🎯 Confiance:</span>
                  <div className="pl-heal-confidence-bar">
                    <div
                      className="pl-heal-confidence-fill"
                      data-level={confidenceLevel}
                      style={{ width: `${confidencePct}%` }}
                    />
                  </div>
                  <span className="pl-heal-confidence-pct">{confidencePct}%</span>
                </div>
              )}

              {/* LLM metadata */}
              <div className="pl-heal-meta">
                {heal.llm_model && (
                  <span className="pl-heal-meta-item">🤖 {heal.llm_model}</span>
                )}
                {heal.llm_duration_ms !== null && (
                  <span className="pl-heal-meta-item">
                    ⏱ {(heal.llm_duration_ms / 1000).toFixed(1)}s
                  </span>
                )}
              </div>
            </div>
          )}

          {/* VP-14: Message confiance faible */}
          {heal.status === "failed" &&
            heal.confidence !== null &&
            heal.confidence < 0.5 && (
              <div className="pl-heal-low-confidence">
                ⚠️ Sensei a diagnostiqué l&apos;erreur, mais le score de confiance (
                {Math.round(heal.confidence * 100)}%) est trop faible pour proposer
                un patch sûr.
              </div>
            )}

          {/* VP-14: Shadow re-run échoué */}
          {heal.status === "failed" &&
            heal.retry_exit_code !== null &&
            heal.retry_exit_code !== 0 && (
              <div className="pl-heal-shadow-failed">
                ❌ Le patch a été appliqué mais le shadow re-run a échoué (exit code{" "}
                {heal.retry_exit_code}). Le code n&apos;a pas été modifié.
              </div>
            )}

          {/* 🔗 MR Link */}
          {heal.mr_id && (
            <div className="pl-heal-section">
              <div className="pl-heal-section-title">🔗 Merge Request Auto-Heal</div>
              <a
                href={`/${owner}/${repo}/mrs`}
                className="pl-heal-mr-link"
              >
                🥷 Voir la MR auto-heal →
              </a>
            </div>
          )}

          {/* 🌿 Shadow Branch */}
          {heal.shadow_branch && (
            <div className="pl-heal-section">
              <div className="pl-heal-section-title">🌿 Branch Shadow</div>
              <code className="pl-heal-diagnosis">{heal.shadow_branch}</code>
            </div>
          )}

          {/* 📜 Shadow Logs (expandable) */}
          {heal.retry_logs && (
            <div className="pl-heal-section">
              <button
                className="pl-heal-logs-toggle"
                onClick={() => setShowLogs(!showLogs)}
              >
                📜 Shadow Logs {showLogs ? "▾" : "▸"}
              </button>
              {showLogs && (
                <pre className="pl-heal-logs-pre">{heal.retry_logs}</pre>
              )}
            </div>
          )}
        </div>
      </div>
    </>
  );
}

// ── VP-12 : renderHunkDiff ──────────────────────────────────
//
// Transforme les chaînes brutes search/replace en lignes diff colorées.
// Le backend envoie des strings avec \n intégrés.
// On split et on préfixe chaque ligne pour un rendu Git-like :
//   search → lignes rouges préfixées "- "
//   replace → lignes vertes préfixées "+ "

function HunkDiff({ hunk }: { hunk: PatchHunk }) {
  const searchLines = hunk.search.split("\n");
  const replaceLines = hunk.replace.split("\n");

  return (
    <div>
      <div className="pl-hunk-file">📄 {hunk.path}</div>
      <div className="pl-hunk-diff">
        {searchLines.map((line, i) => (
          <div key={`s-${i}`} className="pl-hunk-line pl-hunk-line--remove">
            <span className="pl-hunk-prefix">-</span>
            <span>{line}</span>
          </div>
        ))}
        {replaceLines.map((line, i) => (
          <div key={`r-${i}`} className="pl-hunk-line pl-hunk-line--add">
            <span className="pl-hunk-prefix">+</span>
            <span>{line}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
