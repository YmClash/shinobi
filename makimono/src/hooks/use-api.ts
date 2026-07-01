"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getHealth,
  getStatus,
  getOperation,
  getChunksByOperation,
  getOperationDiff,
  getIpfsContent,
  getOperationReviews,
  getScoreHistory,
  listOperations,
  listRepositories,
  getRepository,
  semanticSearch,
  type HealthResponse,
  type SystemStatus,
  type Operation,
  type ChunksResponse,
  type DiffResponse,
  type IpfsContentResponse,
  type OperationsResponse,
  type ReviewsResponse,
  type ScoreHistoryResponse,
  type SemanticSearchResponse,
  type RepositoriesResponse,
  type Repository,
} from "@/lib/api";

// ── Generic async hook ───────────────────────────────────────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function useApi<T>(
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
  autoRefreshMs?: number,
): UseApiState<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await fetcher();
      if (mountedRef.current) {
        setData(result);
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : "Unknown error");
      }
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  useEffect(() => {
    mountedRef.current = true;
    execute();

    let interval: ReturnType<typeof setInterval> | undefined;
    if (autoRefreshMs && autoRefreshMs > 0) {
      interval = setInterval(execute, autoRefreshMs);
    }

    return () => {
      mountedRef.current = false;
      if (interval) clearInterval(interval);
    };
  }, [execute, autoRefreshMs]);

  return { data, error, loading, refetch: execute };
}

// ── Specialized hooks ────────────────────────────────────────

/** Auto-refreshes every 10 seconds */
export function useHealth() {
  return useApi<HealthResponse>(() => getHealth(), [], 10_000);
}

/** Auto-refreshes every 10 seconds */
export function useStatus() {
  return useApi<SystemStatus>(() => getStatus(), [], 10_000);
}

/** Fetches operations list for a specific repo */
export function useOperations(repoPrefix: string, limit = 5) {
  return useApi<OperationsResponse>(() => listOperations(repoPrefix, limit), [repoPrefix, limit]);
}

/** Semantic search with debounce */
export function useSemanticSearch(
  query: string,
  limit = 10,
  threshold = 0.3,
) {
  const [data, setData] = useState<SemanticSearchResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (!query || query.trim().length < 2) {
      setData(null);
      setLoading(false);
      return;
    }

    setLoading(true);

    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    timeoutRef.current = setTimeout(async () => {
      try {
        const result = await semanticSearch(query, limit, threshold);
        if (mountedRef.current) {
          setData(result);
          setError(null);
        }
      } catch (err) {
        if (mountedRef.current) {
          setError(err instanceof Error ? err.message : "Search failed");
        }
      } finally {
        if (mountedRef.current) {
          setLoading(false);
        }
      }
    }, 400);

    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, [query, limit, threshold]);

  const clear = useCallback(() => {
    setData(null);
    setError(null);
    setLoading(false);
  }, []);

  return { data, error, loading, clear };
}

/** Fetches a single operation by ID */
export function useOperation(repoPrefix: string, id: string) {
  return useApi<Operation>(() => getOperation(repoPrefix, id), [repoPrefix, id]);
}

/** Fetches chunks for an operation */
export function useOperationChunks(repoPrefix: string, id: string) {
  return useApi<ChunksResponse>(() => getChunksByOperation(repoPrefix, id), [repoPrefix, id]);
}

/** Fetches diff for an operation */
export function useOperationDiff(repoPrefix: string, id: string) {
  return useApi<DiffResponse>(() => getOperationDiff(repoPrefix, id), [repoPrefix, id]);
}

/** Fetches IPFS content for an operation */
export function useIpfsContent(repoPrefix: string, id: string) {
  return useApi<IpfsContentResponse>(() => getIpfsContent(repoPrefix, id), [repoPrefix, id]);
}

/** Fetches Oracle reviews for an operation — auto-polls every 5 seconds */
export function useOperationReviews(repoPrefix: string, id: string) {
  return useApi<ReviewsResponse>(() => getOperationReviews(repoPrefix, id), [repoPrefix, id], 5_000);
}

/** Fetches score history for the sparkline — auto-refreshes every 15 seconds */
export function useScoreHistory(limit = 10) {
  return useApi<ScoreHistoryResponse>(() => getScoreHistory(limit), [limit], 15_000);
}

// ── Repository hooks (Phase 5 — Forge Sociale) ──────────────

/** Fetches repositories owned by an actor handle */
export function useRepositories(handle: string) {
  return useApi<RepositoriesResponse>(() => listRepositories(handle), [handle]);
}

/** Fetches a single repository by owner/name */
export function useRepository(owner: string, repo: string) {
  return useApi<Repository>(() => getRepository(owner, repo), [owner, repo]);
}
