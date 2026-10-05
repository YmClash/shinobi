"use client";

// ═══════════════════════════════════════════════════════════════
// MR Actions Bar — Merge/Close/Review buttons (Phase 26B)
// Pattern GitHub: overlay plein écran + fermeture au clic extérieur
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import type { MrStatus, MergeStrategy, MrVerdict } from "@/lib/mr-api";
import type { OptimisticMrActions } from "@/hooks/use-mr";

interface MrActionsBarProps {
  status: MrStatus;
  hasConflicts: boolean;
  actions: OptimisticMrActions;
}

type OpenMenu = "none" | "review" | "merge";

export function MrActionsBar({
  status,
  hasConflicts,
  actions,
}: MrActionsBarProps) {
  const [openMenu, setOpenMenu] = useState<OpenMenu>("none");

  const isOpen = status === "open";
  const isDisabled = actions.isPending || !isOpen;

  // Ferme tous les menus
  const closeAll = () => setOpenMenu("none");

  // Toggle un menu (ferme l'autre si ouvert)
  const toggleMenu = (menu: OpenMenu) => {
    setOpenMenu((prev) => (prev === menu ? "none" : menu));
  };

  const handleMerge = async (strategy: MergeStrategy) => {
    closeAll();
    await actions.doMerge(strategy);
  };

  const handleReview = async (verdict: MrVerdict) => {
    closeAll();
    await actions.doReview(verdict);
  };

  const handleClose = async () => {
    closeAll();
    await actions.doClose();
  };

  return (
    <div className="mr-actions-bar">
      {/* ── Error Toast ─────────────────────────────── */}
      {actions.lastError && (
        <div className="mr-action-error animate-fade-in-up">
          <span className="mr-action-error-icon">⚠️</span>
          <span className="mr-action-error-text">{actions.lastError}</span>
        </div>
      )}

      {/* ── Overlay plein écran (pattern GitHub) ───── */}
      {openMenu !== "none" && (
        <div
          className="mr-action-overlay"
          onClick={closeAll}
          aria-hidden="true"
        />
      )}

      <div className="mr-actions-buttons">
        {/* ── Review Button ──────────────────────────── */}
        <div className="mr-action-group">
          <button
            className="mr-action-btn mr-action-review"
            disabled={isDisabled}
            onClick={() => toggleMenu("review")}
          >
            {actions.isPending ? (
              <span className="mr-action-spinner" />
            ) : (
              "📝"
            )}
            Review
          </button>
          {openMenu === "review" && (
            <div className="mr-action-dropdown animate-fade-in-up">
              <button
                className="mr-dropdown-item mr-dropdown-approve"
                onClick={() => handleReview("approve")}
              >
                ✅ Approve
              </button>
              <button
                className="mr-dropdown-item mr-dropdown-changes"
                onClick={() => handleReview("changes_requested")}
              >
                ⚡ Request Changes
              </button>
            </div>
          )}
        </div>

        {/* ── Merge Button ───────────────────────────── */}
        <div className="mr-action-group">
          <button
            className="mr-action-btn mr-action-merge"
            disabled={isDisabled || hasConflicts}
            onClick={() => toggleMenu("merge")}
            title={hasConflicts ? "Cannot merge — conflicts detected" : "Merge this MR"}
          >
            {actions.isPending ? (
              <span className="mr-action-spinner" />
            ) : (
              "🔀"
            )}
            {hasConflicts ? "Conflicts" : "Merge"}
          </button>
          {openMenu === "merge" && !hasConflicts && (
            <div className="mr-action-dropdown animate-fade-in-up">
              <button
                className="mr-dropdown-item"
                onClick={() => handleMerge("fast_forward")}
              >
                ⏩ Fast-Forward
              </button>
              <button
                className="mr-dropdown-item"
                onClick={() => handleMerge("squash")}
              >
                📦 Squash Merge
              </button>
            </div>
          )}
        </div>

        {/* ── Close Button ───────────────────────────── */}
        <button
          className="mr-action-btn mr-action-close"
          disabled={isDisabled}
          onClick={handleClose}
        >
          🚫 Close
        </button>
      </div>

      {/* ── Status Indicators ─────────────────────── */}
      {hasConflicts && isOpen && (
        <div className="mr-conflict-indicator animate-fade-in-up">
          <div className="mr-conflict-title">
            ⚠️ This branch has conflicts that must be resolved before merging
          </div>
          <div className="mr-conflict-guide">
            <p className="mr-conflict-hint">Resolve locally with Jujutsu, then push to update this MR:</p>
            <div className="mr-conflict-steps">
              <code className="mr-conflict-cmd">
                <span className="mr-conflict-step">1.</span> jj rebase -s {"<source>"} -d {"<target>"}
              </code>
              <code className="mr-conflict-cmd">
                <span className="mr-conflict-step">2.</span> # Edit conflicted files, then:
              </code>
              <code className="mr-conflict-cmd">
                <span className="mr-conflict-step">3.</span> jj describe -m &quot;resolve conflicts&quot;
              </code>
              <code className="mr-conflict-cmd">
                <span className="mr-conflict-step">4.</span> jj git push
              </code>
            </div>
            <p className="mr-conflict-footer">
              Once pushed, this MR will refresh automatically and the merge button will become available.
            </p>
          </div>
        </div>
      )}

      {actions.isPending && (
        <div className="mr-action-pending">
          Processing…
        </div>
      )}
    </div>
  );
}
