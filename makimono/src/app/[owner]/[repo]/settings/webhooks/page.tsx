"use client";

// ═══════════════════════════════════════════════════════════════
// Webhooks Settings Page — Phase 34-V4 (Chakra チャクラ)
// /[owner]/[repo]/settings/webhooks
// CRUD dashboard with delivery timeline + payload inspection
// ═══════════════════════════════════════════════════════════════

import { useState, useCallback } from "react";
import { useParams } from "next/navigation";
import { useWebhooks, emitWebhookChanged } from "@/hooks/use-webhooks";
import { deleteWebhook, pingWebhook } from "@/lib/webhook-api";
import type { Webhook } from "@/lib/webhook-api";
import { WebhookFormModal } from "@/components/webhook/webhook-form-modal";
import { WebhookDeliveries } from "@/components/webhook/webhook-deliveries";

// ── Event Category Mapping ───────────────────────────────────

function eventCategory(ev: string): "git" | "mr" | "issue" | "anbu" {
  if (ev === "push") return "git";
  if (ev.startsWith("mr_")) return "mr";
  if (ev.startsWith("issue_")) return "issue";
  if (ev === "anbu_checkpoint") return "anbu";
  return "git";
}

function eventLabel(ev: string): string {
  const labels: Record<string, string> = {
    push: "push",
    mr_created: "mr:created",
    mr_merged: "mr:merged",
    mr_closed: "mr:closed",
    issue_opened: "issue:opened",
    issue_closed: "issue:closed",
    issue_comment: "issue:comment",
    anbu_checkpoint: "anbu:checkpoint",
  };
  return labels[ev] ?? ev;
}

// ── Page Component ───────────────────────────────────────────

export default function WebhooksPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const { data: webhooks, loading, error, refetch } = useWebhooks(params.owner, params.repo);

  // Modal state
  const [showModal, setShowModal] = useState(false);
  const [editWebhook, setEditWebhook] = useState<Webhook | undefined>();

  // Delete confirm
  const [deleteTarget, setDeleteTarget] = useState<Webhook | null>(null);
  const [deleting, setDeleting] = useState(false);

  // Expanded delivery panels (by webhook ID)
  const [expandedDeliveries, setExpandedDeliveries] = useState<Set<string>>(new Set());

  // Ping animation state
  const [pingingId, setPingingId] = useState<string | null>(null);

  // ── Handlers ─────────────────────────────────────────────
  const handleCreate = useCallback(() => {
    setEditWebhook(undefined);
    setShowModal(true);
  }, []);

  const handleEdit = useCallback((wh: Webhook) => {
    setEditWebhook(wh);
    setShowModal(true);
  }, []);

  const handleDelete = useCallback(async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await deleteWebhook(params.owner, params.repo, deleteTarget.id);
      emitWebhookChanged(params.owner, params.repo);
      refetch();
    } catch {
      // Silent fail — the refetch will show current state
    } finally {
      setDeleting(false);
      setDeleteTarget(null);
    }
  }, [deleteTarget, params.owner, params.repo, refetch]);

  const handlePing = useCallback(async (wh: Webhook) => {
    setPingingId(wh.id);
    try {
      await pingWebhook(params.owner, params.repo, wh.id);
    } catch {
      // Silent — the delivery list will show the result
    }
    setTimeout(() => setPingingId(null), 700);
  }, [params.owner, params.repo]);

  const toggleDeliveries = useCallback((whId: string) => {
    setExpandedDeliveries((prev) => {
      const next = new Set(prev);
      if (next.has(whId)) next.delete(whId);
      else next.add(whId);
      return next;
    });
  }, []);

  // ── Render ───────────────────────────────────────────────
  return (
    <div className="wh-page">
      {/* Header */}
      <div className="wh-page-header">
        <h1 className="wh-page-title">🔔 Webhooks</h1>
        <button type="button" className="wh-new-btn" onClick={handleCreate}>
          ✨ New Webhook
        </button>
      </div>

      {/* Loading skeleton */}
      {loading && !webhooks && (
        <div className="wh-skeleton">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="wh-skeleton-row" />
          ))}
        </div>
      )}

      {/* Error */}
      {error && (
        <div style={{ padding: "1rem", fontSize: "0.85rem", color: "oklch(0.7 0.18 25)" }}>
          ⚠️ {error}
        </div>
      )}

      {/* Empty state */}
      {webhooks && webhooks.length === 0 && (
        <div className="wh-empty">
          <span className="wh-empty-icon">🔔</span>
          <p>No webhooks configured yet.</p>
          <p style={{ fontSize: "0.78rem" }}>
            Connect Drone CI, Woodpecker, Jenkins, or any HTTP endpoint to receive real-time events.
          </p>
          <button type="button" className="wh-new-btn" onClick={handleCreate}>
            Create your first webhook
          </button>
        </div>
      )}

      {/* Webhook list */}
      {webhooks && webhooks.length > 0 && (
        <div className="wh-list">
          {webhooks.map((wh) => {
            const isExpanded = expandedDeliveries.has(wh.id);
            const isPinging = pingingId === wh.id;

            return (
              <div key={wh.id} className="wh-card">
                <div className="wh-card-header">
                  <div className="wh-card-left">
                    <span
                      className={`wh-status-dot ${wh.active ? "wh-status-active" : "wh-status-inactive"}`}
                      title={wh.active ? "Active" : "Inactive"}
                    />
                    <div className="wh-card-info">
                      <span className="wh-card-url" title={wh.url}>{wh.url}</span>
                      <div className="wh-card-meta">
                        {/* Delivery status badge */}
                        {wh.last_delivery_at ? (
                          wh.failure_count === 0 ? (
                            <span className="wh-delivery-badge wh-delivery-ok">✅ Healthy</span>
                          ) : (
                            <span className="wh-delivery-badge wh-delivery-fail">
                              ⚠️ {wh.failure_count} failure{wh.failure_count > 1 ? "s" : ""}
                            </span>
                          )
                        ) : (
                          <span className="wh-delivery-badge wh-delivery-none">— No deliveries</span>
                        )}
                        <span>
                          Created {new Date(wh.created_at).toLocaleDateString("fr-FR", {
                            day: "numeric", month: "short", year: "numeric",
                          })}
                        </span>
                      </div>
                    </div>
                  </div>

                  {/* Action buttons */}
                  <div className="wh-card-actions">
                    <button
                      type="button"
                      className={`wh-action-btn wh-btn-ping ${isPinging ? "wh-pinging" : ""}`}
                      onClick={() => handlePing(wh)}
                      title="Send test ping"
                    >
                      🔔 Ping
                    </button>
                    <button
                      type="button"
                      className="wh-action-btn"
                      onClick={() => handleEdit(wh)}
                      title="Edit webhook"
                    >
                      ✏️
                    </button>
                    <button
                      type="button"
                      className="wh-action-btn wh-btn-danger"
                      onClick={() => setDeleteTarget(wh)}
                      title="Delete webhook"
                    >
                      🗑️
                    </button>
                  </div>
                </div>

                {/* Event badges */}
                <div className="wh-events">
                  {wh.events.map((ev) => (
                    <span key={ev} className="wh-event-badge" data-cat={eventCategory(ev)}>
                      {eventLabel(ev)}
                    </span>
                  ))}
                </div>

                {/* Delivery toggle */}
                <button
                  type="button"
                  className="wh-expand-btn"
                  onClick={() => toggleDeliveries(wh.id)}
                >
                  <span className={`wh-expand-icon ${isExpanded ? "wh-expanded" : ""}`}>▶</span>
                  {isExpanded ? "Hide" : "Show"} deliveries
                </button>

                {/* Delivery timeline (lazy-polled) */}
                <WebhookDeliveries
                  owner={params.owner}
                  repo={params.repo}
                  webhookId={wh.id}
                  enabled={isExpanded}
                />
              </div>
            );
          })}
        </div>
      )}

      {/* Create/Edit modal */}
      {showModal && (
        <WebhookFormModal
          owner={params.owner}
          repo={params.repo}
          webhook={editWebhook}
          onClose={() => setShowModal(false)}
          onSaved={refetch}
        />
      )}

      {/* Delete confirmation modal */}
      {deleteTarget && (
        <div className="wh-modal-backdrop" onClick={() => setDeleteTarget(null)}>
          <div className="wh-modal" onClick={(e) => e.stopPropagation()}>
            <h2 className="wh-modal-title">🗑️ Delete Webhook</h2>
            <p className="wh-confirm-text">
              Are you sure you want to delete this webhook?
            </p>
            <p className="wh-confirm-sub" style={{ fontFamily: "var(--font-mono)", wordBreak: "break-all" }}>
              {deleteTarget.url}
            </p>
            <p className="wh-confirm-sub" style={{ marginTop: "0.5rem" }}>
              This action cannot be undone. All delivery history will be lost.
            </p>
            <div className="wh-modal-footer">
              <button
                type="button"
                className="wh-btn-cancel"
                onClick={() => setDeleteTarget(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="wh-btn-submit"
                style={{ background: "oklch(0.5 0.2 25)" }}
                onClick={handleDelete}
                disabled={deleting}
              >
                {deleting ? "⏳ Deleting…" : "Delete Webhook"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
