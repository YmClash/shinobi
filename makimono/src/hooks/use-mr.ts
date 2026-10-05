"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono MR Hooks (Phase 26B)
// SWR cache hooks + React 19 useOptimistic for instant UI
// ═══════════════════════════════════════════════════════════════

import {
  useCallback,
  useEffect,
  useOptimistic,
  useRef,
  useState,
  useTransition,
} from "react";
import {
  type MergeRequest,
  type MrDetail,
  type MrReview,
  type MrStatus,
  type MrVerdict,
  type MergeStrategy,
  type MrDiffResponse,
  listMergeRequests,
  getMergeRequest,
  reviewMergeRequest,
  mergeMergeRequest,
  closeMergeRequest,
  getMergeRequestDiff,
} from "@/lib/mr-api";
import { CacheTTL, getCached, getStale, setCache, invalidateCacheByPrefix } from "@/lib/cache";

// ── Generic SWR hook (same pattern as use-api.ts) ────────────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function useMrApi<T>(
  cacheKey: string,
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
  ttlMs: number = CacheTTL.MEDIUM,
  autoRefreshMs?: number,
): UseApiState<T> {
  const [data, setData] = useState<T | null>(() => {
    if (ttlMs === CacheTTL.NONE) return null;
    return getStale<T>(cacheKey);
  });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(() => {
    if (ttlMs !== CacheTTL.NONE && getCached<T>(cacheKey, ttlMs) !== null) return false;
    return true;
  });
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
    if (ttlMs !== CacheTTL.NONE) {
      const cached = getCached<T>(cacheKey, ttlMs);
      if (cached !== null) {
        if (mountedRef.current) { setData(cached); setLoading(false); setError(null); }
        return;
      }
      const stale = getStale<T>(cacheKey);
      if (stale !== null && mountedRef.current) { setData(stale); setLoading(false); }
    }
    if (mountedRef.current && data === null) setLoading(true);
    setError(null);
    try {
      const result = await fetcher();
      if (mountedRef.current) { setData(result); setError(null); }
      if (ttlMs !== CacheTTL.NONE) setCache(cacheKey, result);
    } catch (err) {
      if (mountedRef.current) setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      if (mountedRef.current) setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cacheKey, ...deps]);

  useEffect(() => {
    mountedRef.current = true;
    execute();
    let interval: ReturnType<typeof setInterval> | undefined;
    if (autoRefreshMs && autoRefreshMs > 0) interval = setInterval(execute, autoRefreshMs);
    return () => { mountedRef.current = false; if (interval) clearInterval(interval); };
  }, [execute, autoRefreshMs]);

  return { data, error, loading, refetch: execute };
}

// ── Specialized MR Hooks ─────────────────────────────────────

/** List MRs with status filter — SHORT cache + auto-refresh 15s. */
export function useMergeRequests(
  owner: string,
  repo: string,
  status?: MrStatus,
  limit = 30,
  offset = 0,
) {
  const state = useMrApi(
    `mrs:${owner}:${repo}:${status ?? "all"}:${limit}:${offset}`,
    () => listMergeRequests(owner, repo, status, limit, offset),
    [owner, repo, status, limit, offset],
    CacheTTL.SHORT,
    15_000,
  );

  // Refetch on custom event (after MR creation/mutation)
  useEffect(() => {
    const onMrChanged = () => {
      invalidateCacheByPrefix(`mrs:${owner}:${repo}:`);
      state.refetch();
    };
    window.addEventListener("shinobi:mr-changed", onMrChanged);
    return () => window.removeEventListener("shinobi:mr-changed", onMrChanged);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.refetch, owner, repo]);

  return state;
}

/** Emit event to trigger MR list refetch globally. */
export function emitMrChanged() {
  window.dispatchEvent(new CustomEvent("shinobi:mr-changed"));
}

/** MR Detail — SHORT cache + polling 30s for conflict detection. */
export function useMergeRequestDetail(owner: string, repo: string, number: number) {
  return useMrApi<MrDetail>(
    `mr:${owner}:${repo}:${number}`,
    () => getMergeRequest(owner, repo, number),
    [owner, repo, number],
    CacheTTL.SHORT,
    30_000,
  );
}

/** MR Diff — LONG cache (immutable once fetched). */
export function useMergeRequestDiff(owner: string, repo: string, number: number) {
  return useMrApi<MrDiffResponse>(
    `mr-diff:${owner}:${repo}:${number}`,
    () => getMergeRequestDiff(owner, repo, number),
    [owner, repo, number],
    CacheTTL.LONG,
  );
}

// ── Optimistic MR Actions (React 19) ─────────────────────────

type OptimisticAction =
  | { type: "merge" }
  | { type: "close" }
  | { type: "review"; verdict: MrVerdict; reviewerId: string };

function optimisticReducer(
  state: MrDetail,
  action: OptimisticAction,
): MrDetail {
  const now = new Date().toISOString();
  switch (action.type) {
    case "merge":
      return {
        ...state,
        status: "merged" as MrStatus,
        merged_at: now,
        updated_at: now,
      };
    case "close":
      return {
        ...state,
        status: "closed" as MrStatus,
        closed_at: now,
        updated_at: now,
      };
    case "review":
      return {
        ...state,
        reviews: [
          ...state.reviews,
          {
            id: `optimistic-${Date.now()}`,
            reviewer_id: action.reviewerId,
            verdict: action.verdict,
            body: null,
            created_at: now,
          },
        ],
        updated_at: now,
      };
  }
}

export interface OptimisticMrActions {
  /** Current MR state (possibly optimistic). */
  optimisticDetail: MrDetail;
  /** Whether a mutation is in flight. */
  isPending: boolean;
  /** Merge the MR (instant UI update + server call). */
  doMerge: (strategy: MergeStrategy) => Promise<boolean>;
  /** Close the MR (instant UI update + server call). */
  doClose: () => Promise<boolean>;
  /** Submit a review (instant UI update + server call). */
  doReview: (verdict: MrVerdict, body?: string) => Promise<boolean>;
  /** Error message from last failed action, if any. */
  lastError: string | null;
}

/**
 * React 19 `useOptimistic` hook for MR mutations.
 *
 * - Updates UI INSTANTLY on merge/close/review
 * - Disables all action buttons while mutation is in-flight (anti-double-click)
 * - Rolls back automatically if the server call fails
 * - Shows toast-style error on failure
 */
export function useOptimisticMrActions(
  owner: string,
  repo: string,
  detail: MrDetail | null,
  onMutated: () => void,
): OptimisticMrActions {
  const [isPending, startTransition] = useTransition();
  const [lastError, setLastError] = useState<string | null>(null);

  // Fallback detail for when data isn't loaded yet
  const safeDetail = detail ?? {
    id: "",
    number: 0,
    title: "",
    description: null,
    source_branch: "",
    target_branch: "",
    status: "open" as MrStatus,
    author_id: "",
    merged_by: null,
    merged_at: null,
    closed_at: null,
    created_at: "",
    updated_at: "",
    has_conflicts: false,
    reviews: [],
    events: [],
  };

  const [optimisticDetail, addOptimistic] = useOptimistic<MrDetail, OptimisticAction>(
    safeDetail,
    optimisticReducer,
  );

  const doMerge = useCallback(
    async (strategy: MergeStrategy): Promise<boolean> => {
      setLastError(null);
      let success = false;
      startTransition(async () => {
        addOptimistic({ type: "merge" });
        try {
          await mergeMergeRequest(owner, repo, safeDetail.number, strategy);
          success = true;
          // Invalidate cache + refetch real data
          invalidateCacheByPrefix(`mr:${owner}:${repo}:`);
          invalidateCacheByPrefix(`mrs:${owner}:${repo}:`);
          emitMrChanged();
          onMutated();
        } catch (err) {
          setLastError(
            err instanceof Error ? err.message : "Merge échoué — veuillez réessayer"
          );
        }
      });
      return success;
    },
    [owner, repo, safeDetail.number, addOptimistic, onMutated],
  );

  const doClose = useCallback(async (): Promise<boolean> => {
    setLastError(null);
    let success = false;
    startTransition(async () => {
      addOptimistic({ type: "close" });
      try {
        await closeMergeRequest(owner, repo, safeDetail.number);
        success = true;
        invalidateCacheByPrefix(`mr:${owner}:${repo}:`);
        invalidateCacheByPrefix(`mrs:${owner}:${repo}:`);
        emitMrChanged();
        onMutated();
      } catch (err) {
        setLastError(
          err instanceof Error ? err.message : "Fermeture échouée — veuillez réessayer"
        );
      }
    });
    return success;
  }, [owner, repo, safeDetail.number, addOptimistic, onMutated]);

  const doReview = useCallback(
    async (verdict: MrVerdict, body?: string): Promise<boolean> => {
      setLastError(null);
      let success = false;
      startTransition(async () => {
        addOptimistic({ type: "review", verdict, reviewerId: "pending" });
        try {
          await reviewMergeRequest(owner, repo, safeDetail.number, { verdict, body });
          success = true;
          invalidateCacheByPrefix(`mr:${owner}:${repo}:`);
          onMutated();
        } catch (err) {
          setLastError(
            err instanceof Error ? err.message : "Review échouée — veuillez réessayer"
          );
        }
      });
      return success;
    },
    [owner, repo, safeDetail.number, addOptimistic, onMutated],
  );

  return { optimisticDetail, isPending, doMerge, doClose, doReview, lastError };
}
