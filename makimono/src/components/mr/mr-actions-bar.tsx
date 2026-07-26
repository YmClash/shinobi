"use client";

// ═══════════════════════════════════════════════════════════════
// MR Actions Bar — Merge/Close/Review buttons (Phase 26B)
// Uses useOptimistic for instant feedback + anti-double-click
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import type { MrStatus, MergeStrategy, MrVerdict } from "@/lib/mr-api";
import type { OptimisticMrActions } from "@/hooks/use-mr";

interface MrActionsBarProps {
  status: MrStatus;
  hasConflicts: boolean;
  actions: OptimisticMrActions;
}

export function MrActionsBar({
  status,
  hasConflicts,
  actions,
}: MrActionsBarProps) {
  const [showMergeMenu, setShowMergeMenu] = useState(false);
  const [showReviewMenu, setShowReviewMenu] = useState(false);

  const isOpen = status === "open";
  const isDisabled = actions.isPending || !isOpen;

  const handleMerge = async (strategy: MergeStrategy) => {
    setShowMergeMenu(false);
    await actions.doMerge(strategy);
  };

  const handleReview = async (verdict: MrVerdict) => {
    setShowReviewMenu(false);
    await actions.doReview(verdict);
  };

  const handleClose = async () => {
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

      <div className="mr-actions-buttons">
        {/* ── Review Button ──────────────────────────── */}
        <div className="mr-action-group">
          <button
            className="mr-action-btn mr-action-review"
            disabled={isDisabled}
            onClick={() => setShowReviewMenu(!showReviewMenu)}
          >
            {actions.isPending ? (
              <span className="mr-action-spinner" />
            ) : (
              "📝"
            )}
            Review
          </button>
          {showReviewMenu && (
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
            onClick={() => setShowMergeMenu(!showMergeMenu)}
            title={hasConflicts ? "Cannot merge — conflicts detected" : "Merge this MR"}
          >
            {actions.isPending ? (
              <span className="mr-action-spinner" />
            ) : (
              "🔀"
            )}
            {hasConflicts ? "Conflicts" : "Merge"}
          </button>
          {showMergeMenu && !hasConflicts && (
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
          ⚠️ This branch has conflicts that must be resolved before merging
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
