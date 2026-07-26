"use client";

// ═══════════════════════════════════════════════════════════════
// MR Review Card — Display a review verdict (Phase 26B)
// ═══════════════════════════════════════════════════════════════

import type { MrReview } from "@/lib/mr-api";

interface MrReviewCardProps {
  review: MrReview;
}

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

export function MrReviewCard({ review }: MrReviewCardProps) {
  const isApproved = review.verdict === "approve";
  const isOptimistic = review.id.startsWith("optimistic-");

  return (
    <div
      className={`mr-review-card ${isOptimistic ? "mr-review-optimistic" : ""}`}
    >
      <div className="mr-review-header">
        <span
          className={`mr-review-verdict ${isApproved ? "mr-verdict-approve" : "mr-verdict-changes"
            }`}
        >
          {isApproved ? "✅ Approved" : "⚡ Changes Requested"}
        </span>
        <span className="mr-review-meta">
          <span className="mr-review-reviewer">
            {isOptimistic ? "You" : review.reviewer_id.slice(0, 8)}
          </span>
          <span className="mr-review-dot">·</span>
          <span className="mr-review-time">
            {formatRelativeTime(review.created_at)}
          </span>
        </span>
      </div>
      {review.body && <div className="mr-review-body">{review.body}</div>}
    </div>
  );
}
