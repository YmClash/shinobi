// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Webhook API Client (Phase 34-V4)
// Typed wrappers for Webhook REST endpoints
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl, buildRepoPrefix } from "./api";
import { authHeaders } from "./auth";

// ── Types ────────────────────────────────────────────────────

export interface Webhook {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  /** Secret HMAC-SHA256 — renvoyé UNIQUEMENT à la création et après régénération. */
  secret?: string | null;
  last_delivery_at: string | null;
  failure_count: number;
  created_at: string;
  updated_at: string;
}

export interface WebhookDelivery {
  id: string;
  event_type: string;
  event_id: string;
  url: string;
  response_status: number | null;
  success: boolean;
  attempt: number;
  duration_ms: number | null;
  error_message: string | null;
  /** JSON du body envoyé (pour l'inspection payload). */
  request_body?: string | null;
  /** JSON du body de la réponse (tronqué à 10KB). */
  response_body?: string | null;
  created_at: string;
}

export interface CreateWebhookRequest {
  url: string;
  events: string[];
  active?: boolean;
}

export interface UpdateWebhookRequest {
  url?: string;
  events?: string[];
  active?: boolean;
}

// ── Helpers ──────────────────────────────────────────────────

const base = () => getBaseUrl();

function hooksPrefix(owner: string, repo: string): string {
  return `${base()}${buildRepoPrefix(owner, repo)}/hooks`;
}

async function handleResponse<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(
      err?.error?.message ?? err?.message ?? `Erreur ${res.status}`
    );
  }
  return res.json();
}

// ── Webhook API Functions ────────────────────────────────────

/** GET /api/v1/repos/{o}/{r}/hooks — Liste les webhooks du dépôt. */
export async function listWebhooks(
  owner: string, repo: string
): Promise<Webhook[]> {
  const res = await fetch(hooksPrefix(owner, repo), { headers: authHeaders() });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/hooks/{id} — Détail d'un webhook. */
export async function getWebhook(
  owner: string, repo: string, id: string
): Promise<Webhook> {
  const res = await fetch(`${hooksPrefix(owner, repo)}/${id}`, { headers: authHeaders() });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/hooks — Créer un webhook. */
export async function createWebhook(
  owner: string, repo: string, data: CreateWebhookRequest
): Promise<Webhook> {
  const res = await fetch(hooksPrefix(owner, repo), {
    method: "POST", headers: authHeaders(), body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** PATCH /api/v1/repos/{o}/{r}/hooks/{id} — Modifier un webhook. */
export async function updateWebhook(
  owner: string, repo: string, id: string, data: UpdateWebhookRequest
): Promise<Webhook> {
  const res = await fetch(`${hooksPrefix(owner, repo)}/${id}`, {
    method: "PATCH", headers: authHeaders(), body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** DELETE /api/v1/repos/{o}/{r}/hooks/{id} — Supprimer un webhook. */
export async function deleteWebhook(
  owner: string, repo: string, id: string
): Promise<{ deleted: boolean }> {
  const res = await fetch(`${hooksPrefix(owner, repo)}/${id}`, {
    method: "DELETE", headers: authHeaders(),
  });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/hooks/{id}/deliveries — Historique des livraisons. */
export async function listDeliveries(
  owner: string, repo: string, id: string
): Promise<WebhookDelivery[]> {
  const res = await fetch(`${hooksPrefix(owner, repo)}/${id}/deliveries`, { headers: authHeaders() });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/hooks/{id}/ping — Ping de test. */
export async function pingWebhook(
  owner: string, repo: string, id: string
): Promise<{ pong: boolean }> {
  const res = await fetch(`${hooksPrefix(owner, repo)}/${id}/ping`, {
    method: "POST", headers: authHeaders(),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/hooks/{id}/regenerate-secret — Régénérer le secret HMAC. */
export async function regenerateSecret(
  owner: string, repo: string, id: string
): Promise<Webhook> {
  const res = await fetch(`${hooksPrefix(owner, repo)}/${id}/regenerate-secret`, {
    method: "POST", headers: authHeaders(),
  });
  return handleResponse(res);
}
