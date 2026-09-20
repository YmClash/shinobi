"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Commit Status Hooks (Phase 39)
// SWR cache hooks for Commit Status API with lazy polling
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import {
  type CombinedStatus,
  getCombinedStatus,
} from "@/lib/commit-status-api";
import { CacheTTL, getCached, getStale, setCache } from "@/lib/cache";

// ── Generic SWR hook (same pattern as use-webhooks.ts) ───────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function useCommitStatusApi<T>(
  cacheKey: string,
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
  ttlMs: number = CacheTTL.MEDIUM,
  autoRefreshMs?: number,
  enabled: boolean = true,
): UseApiState<T> {
  const [data, setData] = useState<T | null>(() => {
    if (!enabled || ttlMs === CacheTTL.NONE) return null;
    return getStale<T>(cacheKey);
  });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(() => {
    if (!enabled) return false;
    if (ttlMs !== CacheTTL.NONE && getCached<T>(cacheKey, ttlMs) !== null) return false;
    return true;
  });
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
    if (!enabled) return;
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
  }, [cacheKey, enabled, ...deps]);

  useEffect(() => {
    mountedRef.current = true;
    execute();
    let interval: ReturnType<typeof setInterval> | undefined;
    if (enabled && autoRefreshMs && autoRefreshMs > 0) interval = setInterval(execute, autoRefreshMs);
    return () => { mountedRef.current = false; if (interval) clearInterval(interval); };
  }, [execute, autoRefreshMs, enabled]);

  return { data, error, loading, refetch: execute };
}

// ── Commit Status Hooks ──────────────────────────────────────

/**
 * Récupère le statut combiné d'un commit avec **lazy polling**.
 *
 * - Polling actif (10s) UNIQUEMENT quand `state === "pending"`
 * - Pas de polling si le build est déjà vert/rouge (éco-conception)
 * - Le hook ne s'active que si `enabled` est true (lazy — quand le
 *   composant badge est visible dans le viewport)
 *
 * @param owner - Handle du propriétaire du dépôt
 * @param repo - Nom du dépôt
 * @param commitId - SHA du commit
 * @param enabled - Activation du hook (lazy visibility)
 */
export function useCombinedStatus(
  owner: string,
  repo: string,
  commitId: string,
  enabled: boolean = true,
) {
  const key = `commit-status:${owner}/${repo}:${commitId}`;
  const result = useCommitStatusApi<CombinedStatus>(
    key,
    () => getCombinedStatus(owner, repo, commitId),
    [owner, repo, commitId],
    CacheTTL.SHORT,
    // Polling 10s uniquement si le statut est "pending"
    // Sinon, pas de polling — le build est terminé
    undefined,
    enabled,
  );

  // Polling intelligent : actif seulement quand pending
  const isPending = result.data?.state === "pending";

  const resultWithPolling = useCommitStatusApi<CombinedStatus>(
    key,
    () => getCombinedStatus(owner, repo, commitId),
    [owner, repo, commitId],
    CacheTTL.NONE, // Skip cache pour le polling actif
    isPending ? 10_000 : undefined,
    enabled && isPending,
  );

  // Si on est en mode polling, utiliser ses données
  // Sinon utiliser le résultat initial caché
  return isPending ? resultWithPolling : result;
}
