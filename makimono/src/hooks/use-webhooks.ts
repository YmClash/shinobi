"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Webhook Hooks (Phase 34-V4)
// SWR cache hooks for Webhooks + lazy delivery polling
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import {
  type Webhook, type WebhookDelivery,
  listWebhooks, listDeliveries,
} from "@/lib/webhook-api";
import { CacheTTL, getCached, getStale, setCache, invalidateCacheByPrefix } from "@/lib/cache";

// ── Generic SWR hook ─────────────────────────────────────────

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function useWebhookApi<T>(
  cacheKey: string,
  fetcher: () => Promise<T>,
  deps: unknown[] = [],
  ttlMs: number = CacheTTL.MEDIUM,
  autoRefreshMs?: number,
  /** Si true, le hook est actif. Sinon il ne fetch pas (lazy polling). */
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

// ── Webhook Hooks ────────────────────────────────────────────

/** List webhooks — SHORT cache + auto-refresh 15s. */
export function useWebhooks(owner: string, repo: string) {
  const key = `webhooks:${owner}/${repo}`;
  return useWebhookApi<Webhook[]>(
    key,
    () => listWebhooks(owner, repo),
    [owner, repo],
    CacheTTL.SHORT,
    15_000,
  );
}

/**
 * List deliveries for a specific webhook — SHORT cache + auto-refresh 10s.
 *
 * **Lazy polling** : le polling ne s'active QUE si `enabled` est true.
 * Cela évite de bombarder l'API pour les webhooks dont le panneau
 * de livraisons n'est pas ouvert.
 */
export function useWebhookDeliveries(
  owner: string, repo: string, webhookId: string,
  enabled: boolean = true,
) {
  const key = `webhook-deliveries:${owner}/${repo}:${webhookId}`;
  return useWebhookApi<WebhookDelivery[]>(
    key,
    () => listDeliveries(owner, repo, webhookId),
    [owner, repo, webhookId],
    CacheTTL.SHORT,
    10_000,
    enabled,
  );
}

/** Invalidate webhook caches after mutation (create/update/delete). */
export function emitWebhookChanged(owner: string, repo: string) {
  invalidateCacheByPrefix(`webhooks:${owner}/${repo}`);
  invalidateCacheByPrefix(`webhook-deliveries:${owner}/${repo}`);
}
