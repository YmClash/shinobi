"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Federation Hooks (Phase 31)
// SWR-cached hooks for federation data with asymmetric TTLs
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getFederationOutbox,
  getFederationFollowers,
  getInboxActivities,
  getNodeInfo,
  type APOrderedCollection,
  type InboxActivitiesResponse,
  type NodeInfoResponse,
} from "@/lib/federation-api";
import { CacheTTL, getCached, getStale, setCache } from "@/lib/cache";
import { useAuth } from "@/hooks/use-auth";
import { getToken } from "@/lib/auth";

// ── Generic async hook (reused from use-api pattern) ────────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function useFedApi<T>(
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
    if (ttlMs !== CacheTTL.NONE && getCached<T>(cacheKey, ttlMs) !== null) {
      return false;
    }
    return true;
  });
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
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
      const stale = getStale<T>(cacheKey);
      if (stale !== null && mountedRef.current) {
        setData(stale);
        setLoading(false);
      }
    }

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

// ── Federation-specific hooks ────────────────────────────────

/**
 * Outbox ActivityPub — SHORT cache, auto-refresh 30s.
 * Public endpoint, no auth needed.
 */
export function useFederationOutbox(handle: string | null) {
  return useFedApi<APOrderedCollection>(
    handle ? `fed:outbox:${handle}` : "__fed_outbox_none__",
    handle
      ? () => getFederationOutbox(handle)
      : () =>
          Promise.resolve({
            "@context": "",
            id: "",
            type: "OrderedCollection" as const,
            totalItems: 0,
            orderedItems: [],
          }),
    [handle],
    CacheTTL.SHORT,
    30_000,
  );
}

/**
 * Followers ActivityPub — MEDIUM cache, auto-refresh 60s.
 * Public endpoint, no auth needed.
 */
export function useFederationFollowers(handle: string | null) {
  return useFedApi<APOrderedCollection>(
    handle ? `fed:followers:${handle}` : "__fed_followers_none__",
    handle
      ? () => getFederationFollowers(handle)
      : () =>
          Promise.resolve({
            "@context": "",
            id: "",
            type: "OrderedCollection" as const,
            totalItems: 0,
            orderedItems: [],
          }),
    [handle],
    CacheTTL.MEDIUM,
    60_000,
  );
}

/**
 * Inbox Activities — SHORT cache, auto-refresh 15s.
 * 🔒 Private: requires JWT. Returns null data if no token.
 */
export function useInboxActivities(handle: string | null, limit = 50) {
  const { user } = useAuth();
  const token = getToken();
  const hasAuth = !!user && !!token;

  return useFedApi<InboxActivitiesResponse>(
    hasAuth && handle
      ? `fed:inbox:${handle}:${limit}`
      : "__fed_inbox_none__",
    hasAuth && handle && token
      ? () => getInboxActivities(handle, token, limit)
      : () => Promise.resolve({ totalItems: 0, items: [] }),
    [handle, limit, hasAuth],
    CacheTTL.SHORT,
    15_000,
  );
}

/**
 * NodeInfo 2.1 — LONG cache (instance metadata changes very rarely).
 */
export function useNodeInfo() {
  return useFedApi<NodeInfoResponse>(
    "fed:nodeinfo",
    () => getNodeInfo(),
    [],
    CacheTTL.LONG,
  );
}

/**
 * Aggregated federation stats for the overview cards.
 */
export interface FederationStats {
  outboxCount: number;
  inboxCount: number;
  followersCount: number;
  softwareVersion: string;
  protocols: string[];
}

export function useFederationStats(handle: string | null): {
  stats: FederationStats;
  loading: boolean;
} {
  const { data: outbox, loading: outLoading } = useFederationOutbox(handle);
  const { data: followers, loading: folLoading } =
    useFederationFollowers(handle);
  const { data: inbox, loading: inLoading } = useInboxActivities(handle);
  const { data: nodeInfo, loading: niLoading } = useNodeInfo();

  const stats: FederationStats = {
    outboxCount: outbox?.totalItems ?? 0,
    inboxCount: inbox?.totalItems ?? 0,
    followersCount: followers?.totalItems ?? 0,
    softwareVersion: nodeInfo?.software?.version ?? "—",
    protocols: nodeInfo?.protocols ?? [],
  };

  return {
    stats,
    loading: outLoading || folLoading || inLoading || niLoading,
  };
}
