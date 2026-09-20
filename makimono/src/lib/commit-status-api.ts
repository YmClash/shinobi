// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Commit Status API Client (Phase 39)
// Typed wrappers for Commit Status REST endpoints
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl, buildRepoPrefix } from "./api";
import { authHeaders } from "./auth";

// ── Types ────────────────────────────────────────────────────

export type CommitStatusState = "pending" | "success" | "failure" | "error";

export interface CommitStatus {
  id: string;
  commit_id: string;
  context: string;
  state: CommitStatusState;
  description: string | null;
  target_url: string | null;
  creator_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface CombinedStatus {
  state: CommitStatusState;
  total_count: number;
  statuses: CommitStatus[];
}

export interface CreateCommitStatusRequest {
  state: CommitStatusState;
  context: string;
  description?: string;
  target_url?: string;
}

// ── Helpers ──────────────────────────────────────────────────

const base = () => getBaseUrl();

function statusesPrefix(owner: string, repo: string, commitId: string): string {
  return `${base()}${buildRepoPrefix(owner, repo)}/statuses/${commitId}`;
}

// ── API Functions ────────────────────────────────────────────

/**
 * POST — Créer ou mettre à jour un statut de commit.
 * UPSERT: si un statut existe déjà pour (repo, commit, context), il est mis à jour.
 */
export async function createCommitStatus(
  owner: string,
  repo: string,
  commitId: string,
  payload: CreateCommitStatusRequest,
): Promise<CommitStatus> {
  const url = statusesPrefix(owner, repo, commitId);
  const res = await fetch(url, {
    method: "POST",
    headers: { ...authHeaders(), "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`POST commit status failed (${res.status}): ${err}`);
  }
  return res.json();
}

/**
 * GET — Lister tous les statuts d'un commit.
 */
export async function listCommitStatuses(
  owner: string,
  repo: string,
  commitId: string,
): Promise<CommitStatus[]> {
  const url = statusesPrefix(owner, repo, commitId);
  const res = await fetch(url, {
    headers: authHeaders(),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`GET commit statuses failed (${res.status}): ${err}`);
  }
  return res.json();
}

/**
 * GET — Récupérer le statut combiné d'un commit.
 * Logique: success si tous success, pending si ≥1 pending, sinon failure.
 */
export async function getCombinedStatus(
  owner: string,
  repo: string,
  commitId: string,
): Promise<CombinedStatus> {
  const url = `${statusesPrefix(owner, repo, commitId)}/combined`;
  const res = await fetch(url, {
    headers: authHeaders(),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`GET combined status failed (${res.status}): ${err}`);
  }
  return res.json();
}
