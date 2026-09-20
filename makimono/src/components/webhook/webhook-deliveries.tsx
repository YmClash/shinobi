"use client";

// ═══════════════════════════════════════════════════════════════
// Webhook Deliveries Timeline — Phase 34-V4 (Chakra チャクラ)
// Shows delivery history + expandable payload inspection
// ═══════════════════════════════════════════════════════════════

import { useState, useCallback } from "react";
import { useWebhookDeliveries } from "@/hooks/use-webhooks";
import type { WebhookDelivery } from "@/lib/webhook-api";

// ── Props ────────────────────────────────────────────────────

interface WebhookDeliveriesProps {
  owner: string;
  repo: string;
  webhookId: string;
  /** Le composant est visible (expanded) — active le polling. */
  enabled: boolean;
}

export function WebhookDeliveries({
  owner, repo, webhookId, enabled,
}: WebhookDeliveriesProps) {
  const { data: deliveries, loading, error } = useWebhookDeliveries(
    owner, repo, webhookId, enabled
  );

  if (!enabled) return null;

  return (
    <div className="wh-deliveries">
      {loading && !deliveries && (
        <div style={{ padding: "0.5rem", fontSize: "0.75rem", color: "var(--muted-foreground)" }}>
          Loading deliveries…
        </div>
      )}

      {error && (
        <div style={{ padding: "0.5rem", fontSize: "0.75rem", color: "oklch(0.7 0.18 25)" }}>
          ⚠️ {error}
        </div>
      )}

      {deliveries && deliveries.length === 0 && (
        <div style={{ padding: "0.75rem", fontSize: "0.78rem", color: "var(--muted-foreground)", textAlign: "center" }}>
          No deliveries yet — try the Ping button!
        </div>
      )}

      {deliveries && deliveries.length > 0 && (
        <div className="wh-delivery-list">
          {deliveries.map((d) => (
            <DeliveryRow key={d.id} delivery={d} />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Delivery Row ─────────────────────────────────────────────

function DeliveryRow({ delivery }: { delivery: WebhookDelivery }) {
  const [expanded, setExpanded] = useState(false);
  const [payloadTab, setPayloadTab] = useState<"request" | "response">("request");

  const toggle = useCallback(() => setExpanded((v) => !v), []);

  const statusClass = getStatusClass(delivery.response_status);
  const icon = delivery.success ? "✅" : "❌";

  return (
    <div>
      <div className="wh-delivery-row" onClick={toggle}>
        <span className="wh-delivery-icon">{icon}</span>
        <div className="wh-delivery-info">
          <div className="wh-delivery-top">
            <span className="wh-delivery-event">{delivery.event_type}</span>
            {delivery.response_status !== null ? (
              <span className={`wh-delivery-status ${statusClass}`}>
                {delivery.response_status}
              </span>
            ) : (
              <span className="wh-delivery-status wh-status-timeout">timeout</span>
            )}
            {delivery.duration_ms !== null && (
              <span className="wh-delivery-duration">{delivery.duration_ms}ms</span>
            )}
            {delivery.attempt > 1 && (
              <span className="wh-delivery-duration">attempt #{delivery.attempt}</span>
            )}
            <span className="wh-delivery-time">
              {formatDeliveryTime(delivery.created_at)}
            </span>
          </div>
          {delivery.error_message && (
            <div className="wh-delivery-error">⚠ {delivery.error_message}</div>
          )}
        </div>
      </div>

      {/* Expandable payload inspection */}
      {expanded && (
        <div className="wh-payload-panel">
          <div className="wh-payload-tabs">
            <button
              type="button"
              className={`wh-payload-tab ${payloadTab === "request" ? "wh-payload-tab-active" : ""}`}
              onClick={() => setPayloadTab("request")}
            >
              📤 Request Body
            </button>
            <button
              type="button"
              className={`wh-payload-tab ${payloadTab === "response" ? "wh-payload-tab-active" : ""}`}
              onClick={() => setPayloadTab("response")}
            >
              📥 Response Body
            </button>
          </div>
          <pre className="wh-payload-code">
            <code>
              {payloadTab === "request"
                ? formatJson(delivery.request_body)
                : formatJson(delivery.response_body)}
            </code>
          </pre>
        </div>
      )}
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────

function getStatusClass(status: number | null): string {
  if (status === null) return "wh-status-timeout";
  if (status >= 200 && status < 300) return "wh-status-2xx";
  if (status >= 300 && status < 400) return "wh-status-3xx";
  if (status >= 400 && status < 500) return "wh-status-4xx";
  return "wh-status-5xx";
}

function formatDeliveryTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString("fr-FR", {
      day: "numeric", month: "short",
      hour: "2-digit", minute: "2-digit", second: "2-digit",
    });
  } catch {
    return iso;
  }
}

function formatJson(raw: string | null | undefined): string {
  if (!raw) return "(empty)";
  try {
    const parsed = JSON.parse(raw);
    return JSON.stringify(parsed, null, 2);
  } catch {
    // Not JSON — display raw
    return raw;
  }
}
