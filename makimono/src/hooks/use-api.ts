"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getHealth,
  getStatus,
  getOperation,
  getChunksByOperation,
  getOperationDiff,
  getDiffContent,
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
  type DiffContentResponse,
  type IpfsContentResponse,
  type OperationsResponse,
  type ReviewsResponse,
  type ScoreHistoryResponse,
  type SemanticSearchResponse,
  type RepositoriesResponse,
  type Repository,
} from "@/lib/api";
import {
  CacheTTL,
  getCached,
  getStale,
  setCache,
  invalidateCacheByPrefix,
} from "@/lib/cache";

// ── Generic async hook with SWR cache ────────────────────────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

/**
 * Hook générique avec cache stale-while-revalidate.
 *
 * - Si les données sont en cache et fraîches (< ttlMs) → retour immédiat, pas de fetch
 * - Si les données sont stale (> ttlMs) → retour immédiat + fetch en arrière-plan
 * - Si aucun cache → fetch normal avec loading=true
 *
 * @param cacheKey   Clé unique pour le cache (ex: "/api/v1/repos/system/momo/operations")
 * @param fetcher    Fonction async qui retourne les données
 * @param deps       Dépendances du hook (déclenchent un refetch si changées)
 * @param ttlMs      Durée de fraîcheur du cache en ms (CacheTTL.SHORT, MEDIUM, LONG, NONE)
 * @param autoRefreshMs  Optionnel — polling interval en ms
 */
function useApi<T>(
  cacheKey: string,
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
  ttlMs: number = CacheTTL.MEDIUM,
  autoRefreshMs?: number,
): UseApiState<T> {
  // Initialise avec les données stale si disponibles
  const [data, setData] = useState<T | null>(() => {
    if (ttlMs === CacheTTL.NONE) return null;
    return getStale<T>(cacheKey);
  });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(() => {
    // Si on a un cache frais, pas de loading
    if (ttlMs !== CacheTTL.NONE && getCached<T>(cacheKey, ttlMs) !== null) {
      return false;
    }
    return true;
  });
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
    // Vérifier le cache frais avant de fetcher
    if (ttlMs !== CacheTTL.NONE) {
      const cached = getCached<T>(cacheKey, ttlMs);
      if (cached !== null) {
        if (mountedRef.current) {
          setData(cached);
          setLoading(false);
          setError(null);
        }
        return;
      }

      // Données stale disponibles → afficher immédiatement, fetch en arrière-plan
      const stale = getStale<T>(cacheKey);
      if (stale !== null && mountedRef.current) {
        setData(stale);
        setLoading(false); // Pas de spinner — données stale visibles
      }
    }

    // Fetch réseau
    if (mountedRef.current && data === null) {
      setLoading(true);
    }
    setError(null);

    try {
      const result = await fetcher();
      if (mountedRef.current) {
        setData(result);
        setError(null);
      }
      // Mettre en cache
      if (ttlMs !== CacheTTL.NONE) {
        setCache(cacheKey, result);
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
  }, [cacheKey, ...deps]);

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

/** Auto-refreshes every 10 seconds — no cache (real-time) */
export function useHealth() {
  return useApi<HealthResponse>("health", () => getHealth(), [], CacheTTL.NONE, 10_000);
}

/** Auto-refreshes every 10 seconds — no cache (real-time) */
export function useStatus() {
  return useApi<SystemStatus>("status", () => getStatus(), [], CacheTTL.NONE, 10_000);
}

/** Fetches operations list for a specific repo — SHORT cache */
export function useOperations(repoPrefix: string, limit = 5) {
  return useApi<OperationsResponse>(
    `ops:${repoPrefix}:${limit}`,
    () => listOperations(repoPrefix, limit),
    [repoPrefix, limit],
    CacheTTL.SHORT,
  );
}

/** Semantic search with debounce — no cache (query-dependent) */
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

/** Fetches a single operation by ID — LONG cache (immutable) */
export function useOperation(repoPrefix: string, id: string) {
  return useApi<Operation>(
    `op:${repoPrefix}:${id}`,
    () => getOperation(repoPrefix, id),
    [repoPrefix, id],
    CacheTTL.LONG,
  );
}

/** Fetches chunks for an operation — LONG cache (immutable after indexing) */
export function useOperationChunks(repoPrefix: string, id: string) {
  return useApi<ChunksResponse>(
    `chunks:${repoPrefix}:${id}`,
    () => getChunksByOperation(repoPrefix, id),
    [repoPrefix, id],
    CacheTTL.LONG,
  );
}

/** Fetches diff for an operation — LONG cache (immutable) */
export function useOperationDiff(repoPrefix: string, id: string) {
  return useApi<DiffResponse>(
    `diff:${repoPrefix}:${id}`,
    () => getOperationDiff(repoPrefix, id),
    [repoPrefix, id],
    CacheTTL.LONG,
  );
}

/** Fetches diff content (line-by-line) for a commit — LONG cache (immutable) */
export function useCommitDiff(repoPrefix: string, id: string) {
  return useApi<DiffContentResponse>(
    `diff-content:${repoPrefix}:${id}`,
    () => getDiffContent(repoPrefix, id),
    [repoPrefix, id],
    CacheTTL.LONG,
  );
}

/** Fetches IPFS content for an operation — LONG cache (immutable via CID) */
export function useIpfsContent(repoPrefix: string, id: string) {
  return useApi<IpfsContentResponse>(
    `ipfs:${repoPrefix}:${id}`,
    () => getIpfsContent(repoPrefix, id),
    [repoPrefix, id],
    CacheTTL.LONG,
  );
}

/** Fetches Oracle reviews for an operation — SHORT cache + auto-poll 5s */
export function useOperationReviews(repoPrefix: string, id: string) {
  return useApi<ReviewsResponse>(
    `reviews:${repoPrefix}:${id}`,
    () => getOperationReviews(repoPrefix, id),
    [repoPrefix, id],
    CacheTTL.SHORT,
    5_000,
  );
}

/** Fetches score history for the sparkline — SHORT cache + auto-refresh 15s */
export function useScoreHistory(limit = 10) {
  return useApi<ScoreHistoryResponse>(
    `scores:${limit}`,
    () => getScoreHistory(limit),
    [limit],
    CacheTTL.SHORT,
    15_000,
  );
}

// ── Repository hooks (Phase 5 — Forge Sociale) ──────────────

/**
 * Fetches repositories owned by an actor handle.
 * - MEDIUM cache (2 min)
 * - Auto-refresh toutes les 30 secondes
 * - Refetch sur visibilitychange (retour onglet)
 * - Refetch sur l'événement custom 'shinobi:repo-created'
 */
export function useRepositories(handle: string | null) {
  const state = useApi<RepositoriesResponse>(
    handle ? `repos:${handle}` : "__repos_none__",
    handle ? () => listRepositories(handle) : () => Promise.resolve({ owner: "", repositories: [], count: 0 }),
    [handle],
    CacheTTL.MEDIUM,
    30_000, // Auto-refresh 30s
  );

  useEffect(() => {
    // Refetch quand l'utilisateur revient sur l'onglet
    const onVisible = () => {
      if (document.visibilityState === "visible") {
        state.refetch();
      }
    };

    // Refetch quand un nouveau dépôt est créé (depuis n'importe quel composant)
    const onRepoCreated = () => {
      // Invalider le cache repos avant de refetch
      invalidateCacheByPrefix("repos:");
      state.refetch();
    };

    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("shinobi:repo-created", onRepoCreated);
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("shinobi:repo-created", onRepoCreated);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.refetch]);

  return state;
}

/**
 * Émet l'événement 'shinobi:repo-created' pour notifier tous les
 * composants qui écoutent (sidebar, forge page, etc.) de refetch.
 */
export function emitRepoCreated() {
  window.dispatchEvent(new CustomEvent("shinobi:repo-created"));
}

/** Fetches a single repository by owner/name — LONG cache */
export function useRepository(owner: string, repo: string) {
  return useApi<Repository>(
    `repo:${owner}:${repo}`,
    () => getRepository(owner, repo),
    [owner, repo],
    CacheTTL.LONG,
  );
}
