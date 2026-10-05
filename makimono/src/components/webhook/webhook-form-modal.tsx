"use client";

// ═══════════════════════════════════════════════════════════════
// Webhook Form Modal — Phase 34-V4 (Chakra チャクラ)
// Create / Edit webhook + Secret display + Regenerate
// ═══════════════════════════════════════════════════════════════

import { useState, useCallback, useEffect } from "react";
import type { Webhook, CreateWebhookRequest, UpdateWebhookRequest } from "@/lib/webhook-api";
import { createWebhook, updateWebhook, regenerateSecret } from "@/lib/webhook-api";
import { emitWebhookChanged } from "@/hooks/use-webhooks";

// ── Event Type Definitions ───────────────────────────────────

interface EventDef {
  value: string;
  label: string;
  cat: "git" | "mr" | "issue" | "anbu";
}

const EVENT_TYPES: EventDef[] = [
  { value: "push",            label: "Push",             cat: "git" },
  { value: "mr_created",      label: "MR Created",       cat: "mr" },
  { value: "mr_merged",       label: "MR Merged",        cat: "mr" },
  { value: "mr_closed",       label: "MR Closed",        cat: "mr" },
  { value: "issue_opened",    label: "Issue Opened",      cat: "issue" },
  { value: "issue_closed",    label: "Issue Closed",      cat: "issue" },
  { value: "issue_comment",   label: "Issue Comment",     cat: "issue" },
  { value: "anbu_checkpoint", label: "ANBU Checkpoint",   cat: "anbu" },
];

const CATEGORIES = [
  { key: "git",   label: "Git" },
  { key: "mr",    label: "Merge Requests" },
  { key: "issue", label: "Issues" },
  { key: "anbu",  label: "ANBU (AI)" },
] as const;

// ── Props ────────────────────────────────────────────────────

interface WebhookFormModalProps {
  owner: string;
  repo: string;
  /** Si défini, on est en mode édition. */
  webhook?: Webhook;
  onClose: () => void;
  onSaved: () => void;
}

export function WebhookFormModal({
  owner, repo, webhook, onClose, onSaved,
}: WebhookFormModalProps) {
  const isEdit = !!webhook;

  const [url, setUrl] = useState(webhook?.url ?? "");
  const [selectedEvents, setSelectedEvents] = useState<Set<string>>(
    new Set(webhook?.events ?? ["push"])
  );
  const [active, setActive] = useState(webhook?.active ?? true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Secret affiché après création ou régénération
  const [revealedSecret, setRevealedSecret] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [regenerating, setRegenerating] = useState(false);

  // Fermer avec Escape
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleEsc);
    return () => window.removeEventListener("keydown", handleEsc);
  }, [onClose]);

  // ── Toggle event ───────────────────────────────────────────
  const toggleEvent = useCallback((ev: string) => {
    setSelectedEvents((prev) => {
      const next = new Set(prev);
      if (next.has(ev)) next.delete(ev);
      else next.add(ev);
      return next;
    });
  }, []);

  // ── Submit ─────────────────────────────────────────────────
  const handleSubmit = useCallback(async () => {
    if (!url.trim()) { setError("URL is required"); return; }
    if (selectedEvents.size === 0) { setError("Select at least one event"); return; }

    setSaving(true);
    setError(null);
    try {
      if (isEdit && webhook) {
        const data: UpdateWebhookRequest = {
          url: url !== webhook.url ? url : undefined,
          events: [...selectedEvents],
          active,
        };
        await updateWebhook(owner, repo, webhook.id, data);
      } else {
        const data: CreateWebhookRequest = { url, events: [...selectedEvents], active };
        const created = await createWebhook(owner, repo, data);
        // Afficher le secret une seule fois
        if (created.secret) {
          setRevealedSecret(created.secret);
          emitWebhookChanged(owner, repo);
          setSaving(false);
          return; // Ne pas fermer — laisser l'utilisateur copier le secret
        }
      }
      emitWebhookChanged(owner, repo);
      onSaved();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setSaving(false);
    }
  }, [url, selectedEvents, active, isEdit, webhook, owner, repo, onSaved, onClose]);

  // ── Regenerate Secret ──────────────────────────────────────
  const handleRegenerate = useCallback(async () => {
    if (!webhook) return;
    setRegenerating(true);
    setError(null);
    try {
      const updated = await regenerateSecret(owner, repo, webhook.id);
      if (updated.secret) setRevealedSecret(updated.secret);
      emitWebhookChanged(owner, repo);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Regenerate failed");
    } finally {
      setRegenerating(false);
    }
  }, [webhook, owner, repo]);

  // ── Copy Secret ────────────────────────────────────────────
  const handleCopy = useCallback(async () => {
    if (!revealedSecret) return;
    await navigator.clipboard.writeText(revealedSecret);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [revealedSecret]);

  // ── Render ─────────────────────────────────────────────────
  return (
    <div className="wh-modal-backdrop" onClick={onClose}>
      <div className="wh-modal" onClick={(e) => e.stopPropagation()}>
        <h2 className="wh-modal-title">
          {isEdit ? "✏️ Edit Webhook" : "✨ New Webhook"}
        </h2>

        {/* URL */}
        <div className="wh-form-group">
          <label className="wh-form-label">Payload URL</label>
          <input
            className={`wh-form-input ${error && !url.trim() ? "wh-input-error" : ""}`}
            type="url"
            placeholder="https://ci.example.com/webhooks/shinobi"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            disabled={!!revealedSecret}
          />
          <p className="wh-form-hint">
            HTTPS required in production. HTTP allowed for localhost in dev mode.
          </p>
        </div>

        {/* Events */}
        <div className="wh-form-group">
          <label className="wh-form-label">Events</label>
          <div className="wh-events-grid">
            {CATEGORIES.map((cat) => (
              <div key={cat.key} style={{ display: "contents" }}>
                <div className="wh-event-group-title">{cat.label}</div>
                {EVENT_TYPES.filter((e) => e.cat === cat.key).map((ev) => (
                  <div key={ev.value} className="wh-event-checkbox">
                    <input
                      type="checkbox"
                      id={`ev-${ev.value}`}
                      checked={selectedEvents.has(ev.value)}
                      onChange={() => toggleEvent(ev.value)}
                      disabled={!!revealedSecret}
                    />
                    <label htmlFor={`ev-${ev.value}`}>{ev.label}</label>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </div>

        {/* Active toggle */}
        <div className="wh-form-group">
          <div className="wh-toggle-row">
            <label className="wh-form-label" style={{ marginBottom: 0 }}>Active</label>
            <button
              type="button"
              className={`wh-toggle ${active ? "wh-toggle-on" : ""}`}
              onClick={() => setActive(!active)}
              disabled={!!revealedSecret}
            />
          </div>
        </div>

        {/* Regenerate Secret (edit mode only) */}
        {isEdit && !revealedSecret && (
          <div className="wh-form-group">
            <label className="wh-form-label">Secret</label>
            <button
              type="button"
              className="wh-regen-btn"
              onClick={handleRegenerate}
              disabled={regenerating}
            >
              {regenerating ? "⏳ Regenerating…" : "🔑 Regenerate Secret"}
            </button>
            <p className="wh-form-hint">
              Generate a new HMAC-SHA256 secret. The old secret will be invalidated immediately.
            </p>
          </div>
        )}

        {/* Secret revealed */}
        {revealedSecret && (
          <div className="wh-secret-box">
            <div className="wh-secret-title">🔐 HMAC Secret</div>
            <div className="wh-secret-value">
              <code>{revealedSecret}</code>
              <button
                type="button"
                className={`wh-copy-btn ${copied ? "wh-copied" : ""}`}
                onClick={handleCopy}
              >
                {copied ? "✅ Copied" : "📋 Copy"}
              </button>
            </div>
            <p className="wh-secret-warning">
              ⚠️ This secret will only be shown once. Copy it now!
            </p>
          </div>
        )}

        {/* Error */}
        {error && (
          <p style={{ color: "oklch(0.7 0.18 25)", fontSize: "0.78rem", marginTop: "0.5rem" }}>
            ⚠️ {error}
          </p>
        )}

        {/* Footer */}
        <div className="wh-modal-footer">
          {revealedSecret ? (
            <button
              type="button"
              className="wh-btn-submit"
              onClick={() => { onSaved(); onClose(); }}
            >
              Done
            </button>
          ) : (
            <>
              <button type="button" className="wh-btn-cancel" onClick={onClose}>
                Cancel
              </button>
              <button
                type="button"
                className="wh-btn-submit"
                onClick={handleSubmit}
                disabled={saving}
              >
                {saving ? "⏳ Saving…" : isEdit ? "Save Changes" : "Create Webhook"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
