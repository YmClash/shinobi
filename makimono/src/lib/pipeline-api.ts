// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Pipeline API Client (Phase 40-C)
// Typed wrappers for Jutsu Runner REST endpoints ⚡
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl, buildRepoPrefix } from "./api";
import { authHeaders } from "./auth";

// ── Types ────────────────────────────────────────────────────

export type PipelineStatus =
  | "queued"
  | "running"
  | "success"
  | "failure"
  | "error"
  | "cancelled";

export type PipelineStageStatus =
  | "pending"
  | "running"
  | "success"
  | "failure"
  | "error"
  | "skipped";

export type TriggerEvent =
  | "push"
  | "manual"
  | "mr_created"
  | "mr_merged";

export interface Pipeline {
  id: string;
  repository_id: string;
  commit_id: string;
  trigger_event: TriggerEvent;
  status: PipelineStatus;
  pipeline_name: string | null;
  started_at: string | null;
  finished_at: string | null;
  duration_ms: number | null;
  creator_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface PipelineStage {
  id: string;
  name: string;
  image: string;
  status: PipelineStageStatus;
  sort_order: number;
  started_at: string | null;
  finished_at: string | null;
  duration_ms: number | null;
  exit_code: number | null;
  /** Logs tronqués par le backend (HEAD 100 + TAIL 200 lignes, cap 64 KB). */
  logs: string | null;
  created_at: string;
  updated_at: string;
}

export interface PipelineListResponse {
  pipelines: Pipeline[];
  total: number;
}

/** Le backend utilise `#[serde(flatten)]` — les champs Pipeline sont aplatis + `stages`. */
export interface PipelineDetailResponse extends Pipeline {
  stages: PipelineStage[];
}

export interface TriggerPipelineBody {
  commit_id: string;
  ref_name?: string;
}

export interface TriggerPipelineResponse {
  status: string;
  pipeline_id: string;
  commit_id: string;
  message: string;
}

// ── Helpers ──────────────────────────────────────────────────

const base = () => getBaseUrl();

function pipelinesPrefix(owner: string, repo: string): string {
  return `${base()}${buildRepoPrefix(owner, repo)}/pipelines`;
}

async function handleResponse<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(
      err?.error?.message ?? err?.message ?? `Erreur ${res.status}`,
    );
  }
  return res.json();
}

// ── API Functions ────────────────────────────────────────────

/** GET /api/v1/repos/{o}/{r}/pipelines — Liste les 30 derniers pipelines. */
export async function listPipelines(
  owner: string,
  repo: string,
): Promise<PipelineListResponse> {
  const res = await fetch(pipelinesPrefix(owner, repo), {
    headers: authHeaders(),
  });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/pipelines/{id} — Détail + stages. */
export async function getPipeline(
  owner: string,
  repo: string,
  id: string,
): Promise<PipelineDetailResponse> {
  const res = await fetch(`${pipelinesPrefix(owner, repo)}/${id}`, {
    headers: authHeaders(),
  });
  return handleResponse(res);
}

/** GET /api/v1/repos/{o}/{r}/pipelines/{id}/stages — Stages seuls (polling). */
export async function listPipelineStages(
  owner: string,
  repo: string,
  pipelineId: string,
): Promise<PipelineStage[]> {
  const res = await fetch(
    `${pipelinesPrefix(owner, repo)}/${pipelineId}/stages`,
    { headers: authHeaders() },
  );
  return handleResponse(res);
}

/** POST /api/v1/repos/{o}/{r}/pipelines/trigger — Déclenchement manuel. */
export async function triggerPipeline(
  owner: string,
  repo: string,
  body: TriggerPipelineBody,
): Promise<TriggerPipelineResponse> {
  const res = await fetch(`${pipelinesPrefix(owner, repo)}/trigger`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify(body),
  });
  return handleResponse(res);
}
