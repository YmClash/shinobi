// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono MR API Client (Phase 26B)
// Typed wrappers for Merge Request REST endpoints
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl, buildRepoPrefix } from "./api";
import { authHeaders } from "./auth";

// ── Types ────────────────────────────────────────────────────

export type MrStatus = "open" | "merged" | "closed";
export type MrVerdict = "approve" | "changes_requested";
export type MergeStrategy = "fast_forward" | "squash";

export interface MergeRequest {
  id: string;
  number: number;
  title: string;
  description: string | null;
  source_branch: string;
  target_branch: string;
  status: MrStatus;
  author_id: string;
  merged_by: string | null;
  merged_at: string | null;
  closed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface MrReview {
  id: string;
  mr_id?: string;
  reviewer_id: string;
  verdict: MrVerdict;
  body: string | null;
  created_at: string;
}

export interface MrEvent {
  id: string;
  actor_id: string;
  event_type: string;
  payload: Record<string, unknown> | null;
  created_at: string;
}

export interface MrDetail extends MergeRequest {
  has_conflicts: boolean;
  reviews: MrReview[];
  events: MrEvent[];
}

export interface MrListResponse {
  items: MergeRequest[];
  total: number;
}

export interface MrDiffFile {
  path: string;
  status: string;
  hunks: Array<{
    old_start: number;
    old_count: number;
    new_start: number;
    new_count: number;
    lines: Array<{
      kind: string;
      content: string;
      old_line: number | null;
      new_line: number | null;
    }>;
  }>;
  additions: number;
  deletions: number;
  too_large: boolean;
}

export interface MrDiffResponse {
  files: MrDiffFile[];
  total_files: number;
}

export interface MergeResult {
  merge_commit_id: string;
  status: string;
}

// ── Helpers ──────────────────────────────────────────────────

const base = () => getBaseUrl();

/** Builds the MR API prefix for a repo. */
function mrPrefix(owner: string, repo: string): string {
  return `${base()}${buildRepoPrefix(owner, repo)}/mrs`;
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

// ── API Functions ────────────────────────────────────────────

/** GET /api/v1/repos/{o}/{r}/mrs — List MRs with optional status filter. */
export async function listMergeRequests(
  owner: string,
  repo: string,
  status?: MrStatus,
  limit = 30,
  offset = 0
): Promise<MrListResponse> {
  const params = new URLSearchParams();
  if (status) params.set("status", status);
  params.set("limit", String(limit));
  params.set("offset", String(offset));

  const res = await fetch(`${mrPrefix(owner, repo)}?${params}`, {
    headers: authHeaders(),
  });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/mrs/{n} — Get MR detail with reviews & events. */
export async function getMergeRequest(
  owner: string,
  repo: string,
  number: number
): Promise<MrDetail> {
  const res = await fetch(`${mrPrefix(owner, repo)}/${number}`, {
    headers: authHeaders(),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/mrs — Create a new MR. */
export async function createMergeRequest(
  owner: string,
  repo: string,
  data: {
    title: string;
    description?: string;
    source_branch: string;
    target_branch: string;
  }
): Promise<MergeRequest> {
  const res = await fetch(mrPrefix(owner, repo), {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/mrs/{n}/reviews — Submit a review. */
export async function reviewMergeRequest(
  owner: string,
  repo: string,
  number: number,
  data: { verdict: MrVerdict; body?: string }
): Promise<MrReview> {
  const res = await fetch(`${mrPrefix(owner, repo)}/${number}/reviews`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify(data),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/mrs/{n}/merge — Merge the MR. */
export async function mergeMergeRequest(
  owner: string,
  repo: string,
  number: number,
  strategy: MergeStrategy = "fast_forward"
): Promise<MergeResult> {
  const res = await fetch(`${mrPrefix(owner, repo)}/${number}/merge`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ strategy }),
  });
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/mrs/{n}/close — Close the MR without merging. */
export async function closeMergeRequest(
  owner: string,
  repo: string,
  number: number
): Promise<{ status: string }> {
  const res = await fetch(`${mrPrefix(owner, repo)}/${number}/close`, {
    method: "POST",
    headers: authHeaders(),
  });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/mrs/{n}/diff — Get MR diff (merge-base aware). */
export async function getMergeRequestDiff(
  owner: string,
  repo: string,
  number: number
): Promise<MrDiffResponse> {
  const res = await fetch(`${mrPrefix(owner, repo)}/${number}/diff`, {
    headers: authHeaders(),
  });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/diff-between?source=X&target=Y — Pre-diff for MR creation. */
export async function getDiffBetween(
  owner: string,
  repo: string,
  source: string,
  target: string
): Promise<MrDiffResponse> {
  const params = new URLSearchParams({ source, target });
  const res = await fetch(
    `${base()}${buildRepoPrefix(owner, repo)}/diff-between?${params}`,
    { headers: authHeaders() }
  );
  return handleResponse(res);
}
