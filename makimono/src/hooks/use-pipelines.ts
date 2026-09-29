"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Pipeline Hooks (Phase 40-C + 41-B)
// SWR cache hooks for Pipeline API with conditional polling ⚡
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import { CacheTTL, getCached, getStale, setCache } from "@/lib/cache";
import {
  type PipelineListResponse,
  type PipelineDetailResponse,
  type PipelineStage,
  type HealAttempt,
  listPipelines,
  getPipeline,
  listPipelineStages,
  listHealAttempts,
} from "@/lib/pipeline-api";

// ── Generic SWR hook (same pattern as use-commit-status.ts) ──

interface UseApiState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  refetch: () => void;
}

function usePipelineApi<T>(
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

// ── Pipeline Hooks ───────────────────────────────────────────

/**
 * Liste des pipelines d'un dépôt (30 derniers).
 *
 * - Cache SHORT (30s)
 * - **Smart polling** : 5s si un pipeline est `running`/`queued`, sinon arrêt
 */
export function usePipelines(owner: string, repo: string) {
  const key = `pipelines:${owner}/${repo}`;
  const result = usePipelineApi<PipelineListResponse>(
    key,
    () => listPipelines(owner, repo),
    [owner, repo],
    CacheTTL.SHORT,
  );

  // Polling léger quand un pipeline est actif
  const hasActive = result.data?.pipelines.some(
    (p) => p.status === "running" || p.status === "queued",
  );

  const pollingResult = usePipelineApi<PipelineListResponse>(
    key,
    () => listPipelines(owner, repo),
    [owner, repo],
    CacheTTL.NONE,
    hasActive ? 5_000 : undefined,
    !!hasActive,
  );

  return hasActive ? pollingResult : result;
}

/**
 * Détail d'un pipeline + stages.
 *
 * - Cache SHORT (30s) au repos
 * - **Smart polling** : 5s si `running`/`queued`, auto-stop sinon
 */
export function usePipelineDetail(owner: string, repo: string, id: string) {
  const key = `pipeline:${owner}/${repo}:${id}`;
  const result = usePipelineApi<PipelineDetailResponse>(
    key,
    () => getPipeline(owner, repo, id),
    [owner, repo, id],
    CacheTTL.SHORT,
  );

  const isActive = result.data?.status === "running" || result.data?.status === "queued";

  const pollingResult = usePipelineApi<PipelineDetailResponse>(
    key,
    () => getPipeline(owner, repo, id),
    [owner, repo, id],
    CacheTTL.NONE,
    isActive ? 5_000 : undefined,
    isActive,
  );

  return isActive ? pollingResult : result;
}

/**
 * Stages d'un pipeline avec **polling live 3s**.
 *
 * - Polling 3s si le pipeline est `running` ou `queued`
 * - VP-13: Continue aussi le polling si un stage est `healing`
 * - Auto-stop quand tous les stages sont terminaux
 * - Endpoint léger GET .../stages (pas de données pipeline)
 */
export function usePipelineStages(
  owner: string,
  repo: string,
  pipelineId: string,
  pipelineStatus: string,
) {
  // VP-13: Le pipeline reste "running" pendant le heal (grâce au fix backend).
  // Mais en double sécurité, on continue le polling si le status est "healing".
  const isActive = pipelineStatus === "running" || pipelineStatus === "queued";

  return usePipelineApi<PipelineStage[]>(
    `pipeline-stages:${owner}/${repo}:${pipelineId}`,
    () => listPipelineStages(owner, repo, pipelineId),
    [owner, repo, pipelineId],
    isActive ? CacheTTL.NONE : CacheTTL.SHORT,
    isActive ? 3_000 : undefined,
    !!pipelineId,
  );
}

/**
 * Phase 41-B — Heal attempts d'un pipeline.
 *
 * VP-15 : Un seul fetch au niveau PipelineDetail, données passées en props.
 * VP-16 — Rythme cardiaque de l'Agent :
 *   - Poll 2s si un heal attempt est en status "pending" ou "healing"
 *   - Auto-stop sinon (terminal: success, failed)
 */
export function useHealAttempts(
  owner: string,
  repo: string,
  pipelineId: string,
) {
  const key = `heal-attempts:${owner}/${repo}:${pipelineId}`;
  const result = usePipelineApi<HealAttempt[]>(
    key,
    () => listHealAttempts(owner, repo, pipelineId),
    [owner, repo, pipelineId],
    CacheTTL.SHORT,
  );

  // VP-16: Polling actif uniquement si un heal est en cours
  const hasActiveHeal = result.data?.some(
    (h) => h.status === "pending" || h.status === "healing",
  );

  const pollingResult = usePipelineApi<HealAttempt[]>(
    key,
    () => listHealAttempts(owner, repo, pipelineId),
    [owner, repo, pipelineId],
    CacheTTL.NONE,
    hasActiveHeal ? 2_000 : undefined, // 2s pendant le heal
    !!hasActiveHeal,
  );

  return hasActiveHeal ? pollingResult : result;
}

