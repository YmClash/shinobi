"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Commit Status Badge (Phase 39 — Le Pont CI/CD) 🌉
// Affiche le statut combiné CI/CD à côté du hash d'un commit.
// ═══════════════════════════════════════════════════════════════

import { useCombinedStatus } from "@/hooks/use-commit-status";
import type { CommitStatusState } from "@/lib/commit-status-api";

// ── Props ────────────────────────────────────────────────────

interface CommitStatusBadgeProps {
  /** Handle du propriétaire du dépôt. */
  owner: string;
  /** Nom du dépôt. */
  repo: string;
  /** SHA du commit (complet ou abrégé — l'API utilise le complet). */
  commitId: string;
  /** Afficher le label textuel ("Success", etc.). Par défaut: false. */
  showLabel?: boolean;
  /** Taille du dot. Par défaut: "sm" (10px). */
  size?: "sm" | "md";
}

// ── State Config ─────────────────────────────────────────────

const STATE_CONFIG: Record<CommitStatusState, { icon: string; label: string }> = {
  pending: { icon: "🟡", label: "Pending" },
  success: { icon: "🟢", label: "Success" },
  failure: { icon: "🔴", label: "Failure" },
  error:   { icon: "⚠️", label: "Error" },
};

// ── Component ────────────────────────────────────────────────

/**
 * Badge d'état CI/CD pour un commit.
 *
 * Affiche un dot coloré (🟢🔴🟡⚠️) avec un tooltip détaillant
 * chaque statut individuel par contexte (ex: "drone/build: success").
 *
 * Le polling est activé automatiquement quand le statut est "pending"
 * et s'arrête quand le build se termine (éco-conception).
 *
 * @example
 * ```tsx
 * <CommitStatusBadge owner="ymclash" repo="shinobi" commitId="abc123" />
 * ```
 */
export function CommitStatusBadge({
  owner,
  repo,
  commitId,
  showLabel = false,
  size = "sm",
}: CommitStatusBadgeProps) {
  const { data, loading, error } = useCombinedStatus(owner, repo, commitId);

  // Pas de statut → ne rien afficher (pas de CI configuré)
  if (!data && !loading) return null;
  if (error && !data) return null;

  // Loading skeleton
  if (loading && !data) {
    return (
      <span className="commit-status-badge" title="Chargement du statut CI...">
        <span className="commit-status-skeleton" />
      </span>
    );
  }

  if (!data) return null;

  const { state, total_count, statuses } = data;
  const config = STATE_CONFIG[state];

  // Ouvrir le target_url du premier statut ayant un lien
  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    const firstWithUrl = statuses.find((s) => s.target_url);
    if (firstWithUrl?.target_url) {
      window.open(firstWithUrl.target_url, "_blank", "noopener,noreferrer");
    }
  };

  return (
    <span
      className="commit-status-badge"
      onClick={handleClick}
      title={`CI: ${config.label} (${total_count} check${total_count !== 1 ? "s" : ""})`}
    >
      {/* Dot coloré */}
      <span
        className={`commit-status-dot commit-status-dot--${state}`}
        style={size === "md" ? { width: 12, height: 12 } : undefined}
      />

      {/* Label optionnel */}
      {showLabel && (
        <span className="commit-status-label">{config.label}</span>
      )}

      {/* Count badge si >1 contexte */}
      {total_count > 1 && (
        <span className="commit-status-count">{total_count}</span>
      )}

      {/* Tooltip au hover */}
      {statuses.length > 0 && (
        <div className="commit-status-tooltip">
          <div className="commit-status-tooltip__header">
            {config.icon} {config.label} — {total_count} check{total_count !== 1 ? "s" : ""}
          </div>
          {statuses.map((s) => (
            <div key={s.id} className="commit-status-tooltip__row">
              <span
                className={`commit-status-dot commit-status-dot--${s.state}`}
                style={{ width: 8, height: 8 }}
              />
              <span className="commit-status-tooltip__context">{s.context}</span>
              <span className={`commit-status-tooltip__state commit-status-tooltip__state--${s.state}`}>
                {s.state}
              </span>
              {s.description && (
                <span className="commit-status-tooltip__description">{s.description}</span>
              )}
            </div>
          ))}
        </div>
      )}
    </span>
  );
}
