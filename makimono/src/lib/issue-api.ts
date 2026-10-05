// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Issue API Client (Phase 33)
// Typed wrappers for Issue/Ticket REST endpoints
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl, buildRepoPrefix } from "./api";
import { authHeaders } from "./auth";

// ── Types ────────────────────────────────────────────────────

export type IssueStatus = "open" | "closed";

export interface Issue {
  id: string;
  number: number;
  title: string;
  body?: string | null;
  status: IssueStatus;
  author_id: string;
  author_handle?: string;
  assignee_id?: string | null;
  closed_by?: string | null;
  closed_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface IssueComment {
  id: string;
  author_id: string;
  author_handle?: string;
  body: string;
  created_at: string;
  updated_at: string;
}

export interface IssueEvent {
  id: string;
  actor_id: string;
  actor_handle?: string;
  event_type: string;
  payload: Record<string, unknown> | null;
  created_at: string;
}

export interface IssueLabel {
  id: string;
  name: string;
  color: string;
  description?: string | null;
}

export interface IssueDetail extends Issue {
  comments: IssueComment[];
  events: IssueEvent[];
  labels: IssueLabel[];
  /** Handles validés par le backend (AST-aware). P1 fix. */
  mentions?: string[];
}

export interface IssueListResponse {
  items: Issue[];
  total: number;
}

// ── Helpers ──────────────────────────────────────────────────

const base = () => getBaseUrl();

function issuePrefix(owner: string, repo: string): string {
  return `${base()}${buildRepoPrefix(owner, repo)}/issues`;
}

function labelPrefix(owner: string, repo: string): string {
  return `${base()}${buildRepoPrefix(owner, repo)}/labels`;
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

// ── Issue API Functions ──────────────────────────────────────

/** GET /api/v1/repos/{o}/{r}/issues */
export async function listIssues(
  owner: string, repo: string,
  status?: IssueStatus, limit = 30, offset = 0
): Promise<IssueListResponse> {
  const params = new URLSearchParams();
  if (status) params.set("status", status);
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  const res = await fetch(`${issuePrefix(owner, repo)}?${params}`, { headers: authHeaders() });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/issues/{n} */
export async function getIssue(
  owner: string, repo: string, number: number
): Promise<IssueDetail> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}`, { headers: authHeaders() });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/issues */
export async function createIssue(
  owner: string, repo: string,
  data: { title: string; body?: string; label_ids?: string[] }
): Promise<Issue> {
  const res = await fetch(issuePrefix(owner, repo), {
    method: "POST", headers: authHeaders(), body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/issues/{n}/close */
export async function closeIssue(
  owner: string, repo: string, number: number
): Promise<{ status: string }> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}/close`, {
    method: "POST", headers: authHeaders(),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/issues/{n}/reopen */
export async function reopenIssue(
  owner: string, repo: string, number: number
): Promise<{ status: string }> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}/reopen`, {
    method: "POST", headers: authHeaders(),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/issues/{n}/comments */
export async function commentIssue(
  owner: string, repo: string, number: number,
  data: { body: string }
): Promise<IssueComment> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}/comments`, {
    method: "POST", headers: authHeaders(), body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/issues/{n}/update */
export async function updateIssue(
  owner: string, repo: string, number: number,
  data: { title?: string; body?: string | null }
): Promise<{ status: string }> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}/update`, {
    method: "POST", headers: authHeaders(), body: JSON.stringify(data),
  });
  return handleResponse(res);
}

// ── Label API Functions ──────────────────────────────────────

/** GET /api/v1/repos/{o}/{r}/labels */
export async function listLabels(
  owner: string, repo: string
): Promise<{ labels: IssueLabel[] }> {
  const res = await fetch(labelPrefix(owner, repo), { headers: authHeaders() });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/labels */
export async function createLabel(
  owner: string, repo: string,
  data: { name: string; color: string; description?: string }
): Promise<IssueLabel> {
  const res = await fetch(labelPrefix(owner, repo), {
    method: "POST", headers: authHeaders(), body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/issues/{n}/labels */
export async function addLabelToIssue(
  owner: string, repo: string, number: number, labelId: string
): Promise<{ status: string }> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}/labels`, {
    method: "POST", headers: authHeaders(), body: JSON.stringify({ label_id: labelId }),
  });
  return handleResponse(res);
}

/** DELETE /api/v1/repos/{o}/{r}/issues/{n}/labels/{labelId} */
export async function removeLabelFromIssue(
  owner: string, repo: string, number: number, labelId: string
): Promise<{ status: string }> {
  const res = await fetch(`${issuePrefix(owner, repo)}/${number}/labels/${labelId}`, {
    method: "DELETE", headers: authHeaders(),
  });
  return handleResponse(res);
}
