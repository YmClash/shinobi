// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Repo Settings API Client (Phase 42-pre-Fix)
// PATCH /api/v1/repos/{owner}/{repo}/settings — Confidentialité IA
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl, buildRepoPrefix } from "./api";
import { authHeaders } from "./auth";

export interface UpdateRepoSettingsRequest {
  /** Autorise l'envoi du code/logs à un fournisseur LLM externe. */
  allow_external_llm?: boolean;
}

export interface RepoSettingsResponse {
  repository_id: string;
  allow_external_llm: boolean;
}

/** PATCH /settings — réservé au owner du dépôt (403 sinon). */
export async function updateRepoSettings(
  owner: string,
  repo: string,
  data: UpdateRepoSettingsRequest
): Promise<RepoSettingsResponse> {
  const res = await fetch(
    `${getBaseUrl()}${buildRepoPrefix(owner, repo)}/settings`,
    { method: "PATCH", headers: authHeaders(), body: JSON.stringify(data) }
  );
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(
      err?.error?.message ?? err?.message ?? `Erreur ${res.status}`
    );
  }
  return res.json();
}
