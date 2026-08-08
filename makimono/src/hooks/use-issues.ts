"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Issue Hooks (Phase 33)
// SWR cache hooks for Issues/Tickets
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import {
  type Issue, type IssueDetail, type IssueStatus, type IssueLabel,
  type IssueListResponse,
  listIssues, getIssue, listLabels,
} from "@/lib/issue-api";
import { CacheTTL, getCached, getStale, setCache, invalidateCacheByPrefix } from "@/lib/cache";

// ── Generic SWR hook ─────────────────────────────────────────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function useIssueApi<T>(
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

// ── Issue Hooks ──────────────────────────────────────────────

/** List issues with status filter — SHORT cache + auto-refresh 15s. */
export function useIssues(
  owner: string, repo: string,
  status?: IssueStatus, limit = 30, offset = 0,
) {
  const key = `issues:${owner}/${repo}:${status ?? "all"}:${limit}:${offset}`;
  return useIssueApi<IssueListResponse>(
    key,
    () => listIssues(owner, repo, status, limit, offset),
    [owner, repo, status, limit, offset],
    CacheTTL.SHORT,
    15_000,
  );
}

/** Get issue detail — SHORT cache + auto-refresh 30s. */
export function useIssueDetail(owner: string, repo: string, number: number) {
  const key = `issue-detail:${owner}/${repo}:${number}`;
  return useIssueApi<IssueDetail>(
    key,
    () => getIssue(owner, repo, number),
    [owner, repo, number],
    CacheTTL.SHORT,
    30_000,
  );
}

/** List labels — MEDIUM cache. */
export function useLabels(owner: string, repo: string) {
  const key = `labels:${owner}/${repo}`;
  return useIssueApi<{ labels: IssueLabel[] }>(
    key,
    () => listLabels(owner, repo),
    [owner, repo],
    CacheTTL.MEDIUM,
  );
}

/** Emit an issue changed event to invalidate caches. */
export function emitIssueChanged(owner: string, repo: string) {
  invalidateCacheByPrefix(`issues:${owner}/${repo}`);
  invalidateCacheByPrefix(`issue-detail:${owner}/${repo}`);
}
