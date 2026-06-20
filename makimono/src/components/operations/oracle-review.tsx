"use client";

import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Skeleton } from "@/components/ui/skeleton";
import { useOperationReviews } from "@/hooks/use-api";
import type { Review } from "@/lib/api";

// ── Composant principal ─────────────────────────────────────

interface OracleReviewProps {
  operationId: string;
}

/**
 * Encart auto-révélé pour les reviews de l'Oracle.
 * Apparaît avec une animation fade-in quand la review est disponible.
 * Polling toutes les 5 secondes via useOperationReviews.
 */
export function OracleReview({ operationId }: OracleReviewProps) {
  const { data, loading } = useOperationReviews(operationId);
  const [isVisible, setIsVisible] = useState(false);

  const hasReviews = data && data.reviews.length > 0;

  // Animation d'apparition : transition smooth quand la review arrive.
  useEffect(() => {
    if (hasReviews && !isVisible) {
      // Petit délai pour que l'animation soit perceptible.
      const timer = setTimeout(() => setIsVisible(true), 100);
      return () => clearTimeout(timer);
    }
  }, [hasReviews, isVisible]);

  // État initial : rien du tout (pas d'encart vide).
  if (!hasReviews && !loading) return null;

  // État de chargement initial (première requête seulement).
  if (loading && !data) {
    return (
      <div className="oracle-review-container oracle-loading">
        <div className="oracle-header">
          <span className="oracle-icon">🔮</span>
          <span className="oracle-title">L&apos;Oracle analyse...</span>
          <span className="oracle-spinner" />
        </div>
      </div>
    );
  }

  // Pas encore de review — en attente silencieuse.
  if (!hasReviews) return null;

  const review = data!.reviews[0]; // La plus récente (triée DESC).

  return (
    <div
      className={`oracle-review-container ${isVisible ? "oracle-visible" : "oracle-hidden"}`}
    >
      {/* ── Header ──────────────────────────── */}
      <div className="oracle-header">
        <span className="oracle-icon">🔮</span>
        <span className="oracle-title">Review de l&apos;Agent Oracle</span>
        <div className="oracle-badges">
          <ModelBadge model={review.model} />
          <DurationBadge durationMs={review.duration_ms} />
          {review.score !== null && <ScoreBadge score={review.score} />}
        </div>
      </div>

      {/* ── Summary ─────────────────────────── */}
      <div className="oracle-summary">{review.summary}</div>

      {/* ── Content (Markdown) ──────────────── */}
      <div className="oracle-content">
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          components={{
            h2: ({ children }) => (
              <h3 className="oracle-md-h2">{children}</h3>
            ),
            h3: ({ children }) => (
              <h4 className="oracle-md-h3">{children}</h4>
            ),
            ul: ({ children }) => (
              <ul className="oracle-md-ul">{children}</ul>
            ),
            ol: ({ children }) => (
              <ol className="oracle-md-ol">{children}</ol>
            ),
            li: ({ children }) => (
              <li className="oracle-md-li">{children}</li>
            ),
            code: ({ children, className }) => {
              const isInline = !className;
              return isInline ? (
                <code className="oracle-md-code-inline">{children}</code>
              ) : (
                <code className={`oracle-md-code-block ${className ?? ""}`}>
                  {children}
                </code>
              );
            },
            pre: ({ children }) => (
              <pre className="oracle-md-pre">{children}</pre>
            ),
            strong: ({ children }) => (
              <strong className="oracle-md-strong">{children}</strong>
            ),
            p: ({ children }) => (
              <p className="oracle-md-p">{children}</p>
            ),
          }}
        >
          {review.content}
        </ReactMarkdown>
      </div>

      {/* ── Footer (timestamp) ──────────────── */}
      <div className="oracle-footer">
        <span>
          Généré le{" "}
          {new Date(review.created_at).toLocaleString("fr-FR", {
            day: "2-digit",
            month: "short",
            year: "numeric",
            hour: "2-digit",
            minute: "2-digit",
          })}
        </span>
      </div>
    </div>
  );
}

// ── Sous-composants (Badges) ────────────────────────────────

function ModelBadge({ model }: { model: string }) {
  return (
    <span className="oracle-badge oracle-badge-model">
      🦙 {model}
    </span>
  );
}

function DurationBadge({ durationMs }: { durationMs: number }) {
  const seconds = (durationMs / 1000).toFixed(1);
  return (
    <span className="oracle-badge oracle-badge-duration">
      ⏱️ {seconds}s
    </span>
  );
}

function ScoreBadge({ score }: { score: number }) {
  const percent = Math.round(score * 100);
  const color =
    score >= 0.8
      ? "oracle-score-green"
      : score >= 0.5
        ? "oracle-score-yellow"
        : "oracle-score-red";
  return (
    <span className={`oracle-badge oracle-badge-score ${color}`}>
      {percent}%
    </span>
  );
}

// ── Loading skeleton ────────────────────────────────────────

export function OracleReviewSkeleton() {
  return (
    <div className="oracle-review-container oracle-loading">
      <div className="oracle-header">
        <Skeleton className="h-5 w-5 rounded-full" />
        <Skeleton className="h-4 w-48" />
      </div>
      <Skeleton className="h-3 w-3/4 mt-2" />
      <div className="mt-3 space-y-2">
        <Skeleton className="h-3 w-full" />
        <Skeleton className="h-3 w-5/6" />
        <Skeleton className="h-3 w-4/6" />
      </div>
    </div>
  );
}
