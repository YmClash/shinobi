// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono API Client
// Typed wrappers for the Taijutsu REST API
// ═══════════════════════════════════════════════════════════════

// ── Repo Prefix Helper (Phase 5C — URL-Driven) ──────────────
// Le prefix est désormais calculé à partir de l'URL (params.owner, params.repo).
// Plus de constante hardcodée — chaque page passe son propre prefix.

/**
 * Retourne le base URL pour les appels API.
 * - Côté navigateur : chaîne vide (URL relative, ex: "/api/v1/...")
 * - Côté serveur Next.js (RSC / SSR) : URL absolue car Node.js
 *   ne comprend pas les URLs relatives.
 *
 * Priorité : window (client) > NEXT_PUBLIC_APP_URL > port 3001 (dev fallback)
 */
export function getBaseUrl(): string {
  // Client-side : URL relative suffit (même origine)
  if (typeof window !== "undefined") return "";

  // Variable explicite définie dans .env.local (dev) ou les vars d'env de prod
  if (process.env.NEXT_PUBLIC_APP_URL) {
    return process.env.NEXT_PUBLIC_APP_URL;
  }

  // Fallback dev — port hardcodé dans package.json "next dev --port 3001"
  return "http://localhost:3001";
}

/** Construit le prefix API pour un dépôt donné. */
export function buildRepoPrefix(owner: string, repo: string): string {
  return `/api/v1/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`;
}

// ── Types ────────────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  service: string;
  version: string;
}

export interface SystemStatus {
  service: string;
  version: string;
  components: Record<string, string>;
  status: string;
}

export interface Operation {
  id: string;
  author_id: string;
  repository_id: string;
  content_id: string;
  ipfs_content_id?: string;
  description: string;
  parent_ids: string[];
  created_at: string;
}

export interface Chunk {
  kind: string;
  name?: string;
  content: string;
  start_line: number;
  end_line: number;
  file_path: string;
  language: string;
}

export interface SemanticChunk extends Chunk {
  similarity: number;
}

export interface OperationsResponse {
  operations: Operation[];
  count: number;
  /** Nombre total absolu d'opérations dans le dépôt (Phase 17). */
  total_count?: number;
}

export interface ChunksResponse {
  chunks: Chunk[];
  count: number;
  operation_id?: string;
}

export interface SemanticSearchResponse {
  query: string;
  chunks: SemanticChunk[];
  count: number;
}

export interface DiffResponse {
  operation_id: string;
  content_id: string;
  changed_files: string[];
  count: number;
}

// ── Phase 17 — Diff Colorisé (line-by-line) ───────────────────

export type DiffLineKind = "add" | "remove" | "context";
export type DiffStatusKind = "added" | "modified" | "deleted";

export interface DiffLine {
  kind: DiffLineKind;
  content: string;
  old_line: number | null;
  new_line: number | null;
}

export interface DiffHunk {
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  status: DiffStatusKind;
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
  too_large: boolean;
}

export interface DiffContentResponse {
  operation_id: string;
  files: FileDiff[];
  stats: {
    files_changed: number;
    additions: number;
    deletions: number;
  };
}

export interface IpfsFile {
  path: string;
  size: number;
  content: string;
  language: string | null;
  /** Per-file IPFS CID (Phase 8.1 Merkle DAG only; null for legacy blobs). */
  cid: string | null;
}

export interface IpfsContentResponse {
  operation_id: string;
  ipfs_cid: string;
  blob_size: number;
  files: IpfsFile[];
  count: number;
}

// ── Oracle Review Types (Phase 9) ────────────────────────────

export interface Review {
  id: string;
  reviewer: string;
  model: string;
  summary: string;
  content: string;
  score: number | null;
  duration_ms: number;
  created_at: string;
}

export interface ReviewsResponse {
  operation_id: string;
  reviews: Review[];
  count: number;
}

// ── Score History Types (Phase 9.2 — Sparkline) ──────────────

export interface ScorePoint {
  operation_id: string;
  score: number;
  created_at: string;
}

export interface ScoreHistoryResponse {
  scores: ScorePoint[];
  count: number;
  average: number | null;
  trend: "rising" | "falling" | "stable";
}

// ── Repository Types (Phase 5 — Forge Sociale) ───────────────

export interface Repository {
  id: string;
  owner_id: string;
  name: string;
  display_name: string;
  description: string | null;
  visibility: string;
  default_branch: string;
  created_at: string;
  /** URL Git source pour les repos importés depuis GitHub (Phase 19B). */
  mirror_source_url?: string | null;
  /** Timestamp du dernier import miroir (Phase 19B). */
  mirror_synced_at?: string | null;
  /** UUID du dépôt parent si c'est un fork (Phase 37B). */
  forked_from_id?: string | null;
  /** Nombre de forks de ce repo (Phase 37B). */
  fork_count?: number;
  /** Handle du propriétaire du repo parent (Phase 37B). */
  forked_from_owner?: string | null;
  /** Nom (slug) du repo parent (Phase 37B). */
  forked_from_name?: string | null;
}

export interface RepositoriesResponse {
  owner: string;
  repositories: Repository[];
  count: number;
}

export interface CreateRepositoryRequest {
  owner_id: string;
  name: string;
  display_name: string;
  description?: string;
  visibility?: string;
}

// ── API Error ────────────────────────────────────────────────

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

// ── Fetch helper ─────────────────────────────────────────────

async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  // Préfixe l'URL avec le base URL si on est côté serveur
  const url = `${getBaseUrl()}${path}`;
  const res = await fetch(url, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...options?.headers,
    },
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new ApiError(res.status, text);
  }

  return res.json() as Promise<T>;
}

// ── API Functions ────────────────────────────────────────────

export async function getHealth(): Promise<HealthResponse> {
  return apiFetch<HealthResponse>("/health");
}

export async function getStatus(): Promise<SystemStatus> {
  return apiFetch<SystemStatus>("/api/v1/status");
}

export async function listOperations(repoPrefix: string, limit = 50): Promise<OperationsResponse> {
  return apiFetch<OperationsResponse>(`${repoPrefix}/operations?limit=${limit}`);
}

export async function getOperation(repoPrefix: string, id: string): Promise<Operation> {
  return apiFetch<Operation>(`${repoPrefix}/operations/${id}`);
}

export async function getChunksByOperation(
  repoPrefix: string,
  id: string,
  file?: string,
): Promise<ChunksResponse> {
  const params = file ? `?file=${encodeURIComponent(file)}` : "";
  return apiFetch<ChunksResponse>(`${repoPrefix}/operations/${id}/chunks${params}`);
}

export async function searchChunksByName(
  name: string,
): Promise<{ query: string; chunks: Chunk[]; count: number }> {
  return apiFetch(`/api/v1/chunks/search?name=${encodeURIComponent(name)}`);
}

export async function semanticSearch(
  query: string,
  limit = 10,
  threshold = 0.3,
): Promise<SemanticSearchResponse> {
  return apiFetch<SemanticSearchResponse>("/api/v1/chunks/semantic-search", {
    method: "POST",
    body: JSON.stringify({ query, limit, threshold }),
  });
}

export async function getOperationDiff(
  repoPrefix: string,
  id: string,
): Promise<DiffResponse> {
  return apiFetch<DiffResponse>(`${repoPrefix}/operations/${id}/diff`);
}

/** Récupère le diff ligne par ligne (Phase 17 — Diff Colorisé). */
export async function getDiffContent(
  repoPrefix: string,
  id: string,
): Promise<DiffContentResponse> {
  return apiFetch<DiffContentResponse>(`${repoPrefix}/operations/${id}/diff-content`);
}

export async function getIpfsContent(
  repoPrefix: string,
  id: string,
): Promise<IpfsContentResponse> {
  return apiFetch<IpfsContentResponse>(`${repoPrefix}/operations/${id}/ipfs`);
}

// ── Create Operation ─────────────────────────────────────────

export interface FileEntry {
  path: string;
  content_b64: string;
}

export interface CreateOperationRequest {
  author_id: string;
  description: string;
  parent_ids?: string[];
  files: FileEntry[];
  /** Phase 10C: identifiant du dépôt cible (UUID). Optionnel — utilise DEFAULT_REPO_ID si omis. */
  repository_id?: string;
}

export async function createOperation(
  repoPrefix: string,
  body: CreateOperationRequest,
): Promise<Operation> {
  const res = await fetch(`${repoPrefix}/operations`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new ApiError(res.status, text);
  }

  return res.json() as Promise<Operation>;
}

// ── Oracle Reviews (Phase 9) ─────────────────────────────────

export async function getOperationReviews(
  repoPrefix: string,
  id: string,
): Promise<ReviewsResponse> {
  return apiFetch<ReviewsResponse>(`${repoPrefix}/operations/${id}/reviews`);
}

// ── Score History (Phase 9.2 — Sparkline) ────────────────────

export async function getScoreHistory(limit = 10): Promise<ScoreHistoryResponse> {
  return apiFetch<ScoreHistoryResponse>(`/api/v1/reviews/scores?limit=${limit}`);
}

// ── Repository API (Phase 5 — Forge Sociale) ─────────────────

/** Liste les dépôts d'un acteur par handle. */
export async function listRepositories(
  handle: string,
): Promise<RepositoriesResponse> {
  return apiFetch<RepositoriesResponse>(`/api/v1/actors/${encodeURIComponent(handle)}/repos`);
}

/** Récupère les métadonnées d'un dépôt par owner/name. */
export async function getRepository(
  owner: string,
  repo: string,
): Promise<Repository> {
  return apiFetch<Repository>(`/api/v1/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`);
}

/** Crée un nouveau dépôt. Retourne 201 Created ou 409 Conflict. */
export async function createRepository(
  body: CreateRepositoryRequest,
): Promise<Repository> {
  return apiFetch<Repository>("/api/v1/repos", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

// ── Phase 24 — Soft Delete (Corbeille) ───────────────────────────

export interface TrashRepository {
  id: string;
  name: string;
  display_name: string;
  description: string | null;
  visibility: string;
  deleted_at: string;
  seconds_until_purge: number;
}

export interface TrashResponse {
  owner: string;
  trash: TrashRepository[];
  count: number;
  retention_seconds: number;
}

/** Met un dépôt en corbeille (soft delete). Requiert un JWT valide. */
export async function archiveRepository(
  owner: string,
  repo: string,
  confirmationWord: string,
  expectedWord: string,
  token: string,
): Promise<void> {
  const url = `${getBaseUrl()}/api/v1/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/archive`;
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      confirmation_word: confirmationWord,
      expected_word: expectedWord,
    }),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
}

/** Restaure un dépôt depuis la corbeille. Requiert un JWT valide. */
export async function restoreRepository(
  owner: string,
  repo: string,
  token: string,
): Promise<Repository> {
  const url = `${getBaseUrl()}/api/v1/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/restore`;
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
  return res.json() as Promise<Repository>;
}

/** Liste les dépôts en corbeille d'un utilisateur. Requiert un JWT valide. */
export async function listTrashRepositories(
  handle: string,
  token: string,
): Promise<TrashResponse> {
  const url = `${getBaseUrl()}/api/v1/actors/${encodeURIComponent(handle)}/trash`;
  const res = await fetch(url, {
    headers: {
      Authorization: `Bearer ${token}`,
    },
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
  return res.json() as Promise<TrashResponse>;
}

// ── Sensei Agent (Phase 15 — 先生) ─────────────────────────────

/** Message dans l'historique de conversation. */
export interface SenseiMessage {
  role: "user" | "assistant";
  content: string;
}

// ── Sensei Models & Warmup ──────────────────────────────────────────

/** Modèle installé sur Ollama Sensei. */
export interface SenseiModelInfo {
  name: string;
  size: number;
}

/** Réponse de GET /api/v1/sensei/models. */
export interface SenseiModelsResponse {
  models: SenseiModelInfo[];
  active: string;
}

/** Récupère la liste des modèles installés sur Ollama #2 (Sensei). 🔒 Auth requise. */
export async function getSenseiModels(token?: string | null): Promise<SenseiModelsResponse> {
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  const res = await fetch(`${getBaseUrl()}/api/v1/sensei/models`, { headers });
  if (!res.ok) throw new Error(`Models fetch failed: ${res.status}`);
  return res.json();
}

/** Pré-charge un modèle dans la RAM d'Ollama (élimine le cold-start ~30s). 🔒 Auth requise. */
export async function warmupSenseiModel(model: string, token?: string | null): Promise<void> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;
  const res = await fetch(`${getBaseUrl()}/api/v1/sensei/warmup`, {
    method: "POST",
    headers,
    body: JSON.stringify({ model }),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`Warmup failed (${res.status}): ${text}`);
  }
}

/** Corps de la requête POST /api/v1/sensei/chat. */
export interface SenseiChatRequest {
  query: string;
  file_path: string;
  file_content?: string;
  language: string | null;
  owner: string;
  repo: string;
  history: SenseiMessage[];
}

/** Source RAG trouvée par Tensai. */
export interface SenseiSource {
  file_path: string;
  name: string | null;
  similarity: number;
  language: string;
  start_line: number;
  end_line: number;
}

/** Événement SSE reçu du stream Sensei. */
export type SenseiStreamEvent =
  | { type: "context"; sources: SenseiSource[]; oracle_score: number | null; oracle_summary: string | null }
  | { type: "token"; content: string }
  | { type: "done"; model: string; duration_ms: number }
  | { type: "error"; message: string };

/** Callbacks pour le stream SSE Sensei. */
export interface SenseiStreamCallbacks {
  onContext: (sources: SenseiSource[], oracleScore: number | null, oracleSummary: string | null) => void;
  onToken: (token: string) => void;
  onDone: (model: string, durationMs: number) => void;
  onError: (error: string) => void;
}

/**
 * Ouvre un stream SSE vers l'agent Sensei (先生).
 *
 * Utilise fetch + ReadableStream (pas EventSource, car POST avec body).
 * Retourne un AbortController pour permettre l'annulation (bouton Stop).
 *
 * @example
 * ```ts
 * const controller = senseiChatStream(request, {
 *   onContext: (sources, score, summary) => setSources(sources),
 *   onToken: (token) => setContent(prev => prev + token),
 *   onDone: (model, ms) => setDone(true),
 *   onError: (err) => setError(err),
 * });
 *
 * // Pour annuler :
 * controller.abort();
 * ```
 */
export function senseiChatStream(
  body: SenseiChatRequest,
  callbacks: SenseiStreamCallbacks,
  token?: string | null,
): AbortController {
  const controller = new AbortController();
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  fetch(`${getBaseUrl()}/api/v1/sensei/chat`, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
    signal: controller.signal,
  })
    .then(async (response) => {
      if (!response.ok) {
        const text = await response.text().catch(() => "Unknown error");
        callbacks.onError(`HTTP ${response.status}: ${text}`);
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        callbacks.onError("No readable stream available");
        return;
      }

      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        // Parse SSE events (format: "data: {...}\n\n")
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? ""; // Keep incomplete line in buffer

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed === ":") continue; // SSE comment / keep-alive

          if (trimmed.startsWith("data:")) {
            const jsonStr = trimmed.slice(5).trim();
            if (!jsonStr) continue;

            try {
              const event = JSON.parse(jsonStr) as SenseiStreamEvent;

              switch (event.type) {
                case "context":
                  callbacks.onContext(event.sources, event.oracle_score, event.oracle_summary);
                  break;
                case "token":
                  callbacks.onToken(event.content);
                  break;
                case "done":
                  callbacks.onDone(event.model, event.duration_ms);
                  break;
                case "error":
                  callbacks.onError(event.message);
                  break;
              }
            } catch {
              // Skip unparseable lines (keep-alive, etc.)
            }
          }
        }
      }
    })
    .catch((err) => {
      if (err instanceof DOMException && err.name === "AbortError") {
        // User cancelled — normal behavior
        return;
      }
      callbacks.onError(String(err));
    });

  return controller;
}

// ── ANBU Checkpoints (Phase 28C) ─────────────────────────────

/** An AI checkpoint synced from the ANBU CLI. */
export interface Checkpoint {
  id: string;
  agent: string;
  session_id: string;
  message: string | null;
  commit_id: string | null;
  ipfs_cid: string;
  artifact_count: number;
  total_size: number;
  created_at: string;
}

export interface CheckpointsResponse {
  owner: string;
  repo: string;
  checkpoints: Checkpoint[];
  count: number;
}

/** Liste les checkpoints ANBU d'un dépôt. */
export async function listCheckpoints(
  repoPrefix: string,
): Promise<CheckpointsResponse> {
  return apiFetch<CheckpointsResponse>(`${repoPrefix}/checkpoints`);
}

/** Récupère le détail d'un checkpoint ANBU. */
export async function getCheckpoint(
  repoPrefix: string,
  id: string,
): Promise<Checkpoint> {
  return apiFetch<Checkpoint>(`${repoPrefix}/checkpoints/${id}`);
}

// ── Phase 37A — Actor Profile (Le Visage Public) ─────────────

/** Activité récente de l'outbox ActivityPub. */
export interface ProfileActivity {
  type: string;
  object_type: string;
  published: string;
  object_id: string;
}

/** Stats publiques d'un acteur. */
export interface ActorStats {
  public_repos: number;
  total_repos: number;
  bots_count: number;
  follower_count: number;
}

/** Profil public d'un acteur (humain, bot, ou système). */
export interface ActorProfile {
  actor: {
    id: string;
    handle: string;
    display_name: string;
    actor_type: string;
    avatar_url: string | null;
    bio: string | null;
    created_at: string | null;
  };
  stats: ActorStats;
  fediverse_address: string;
  is_followed_by_current_user: boolean;
  recent_activities: ProfileActivity[];
  parent: {
    id: string;
    handle: string;
    display_name: string;
    avatar_url: string | null;
  } | null;
  is_system?: boolean;
}

/** Récupère le profil public d'un acteur par handle. */
export async function getActorProfile(
  handle: string,
  token?: string | null,
): Promise<ActorProfile> {
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  return apiFetch<ActorProfile>(
    `/api/v1/actors/${encodeURIComponent(handle)}/profile`,
    { headers },
  );
}

// ── Phase 37B — Fork Local (Le Dédoublement) ────────────────

/** Fork un dépôt dans le namespace de l'utilisateur authentifié. */
export async function forkRepository(
  owner: string,
  repo: string,
  token: string,
): Promise<Repository> {
  return apiFetch<Repository>(
    `/api/v1/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}/fork`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
    },
  );
}
