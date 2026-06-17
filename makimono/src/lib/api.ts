// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono API Client
// Typed wrappers for the Taijutsu REST API
// ═══════════════════════════════════════════════════════════════

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
  const res = await fetch(path, {
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

export async function listOperations(limit = 50): Promise<OperationsResponse> {
  return apiFetch<OperationsResponse>(`/api/v1/operations?limit=${limit}`);
}

export async function getOperation(id: string): Promise<Operation> {
  return apiFetch<Operation>(`/api/v1/operations/${id}`);
}

export async function getChunksByOperation(
  id: string,
  file?: string,
): Promise<ChunksResponse> {
  const params = file ? `?file=${encodeURIComponent(file)}` : "";
  return apiFetch<ChunksResponse>(`/api/v1/operations/${id}/chunks${params}`);
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
  id: string,
): Promise<DiffResponse> {
  return apiFetch<DiffResponse>(`/api/v1/operations/${id}/diff`);
}
