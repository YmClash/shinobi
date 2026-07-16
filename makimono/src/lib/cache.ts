// ═══════════════════════════════════════════════════════════════
// SHINOBI — Client-side Cache (Phase 18)
//
// Cache en mémoire avec TTL (stale-while-revalidate pattern).
// Évite les requêtes réseau redondantes lorsqu'on navigue entre
// les pages (ex: retour arrière, switch d'onglets operations).
//
// Architecture :
//   Map<cacheKey, { data, timestamp }> + TTL configurable
//   - Cache HIT : retourne immédiatement les données (stale)
//   - Background revalidation : lance le fetch en arrière-plan
//   - Cache MISS : fetch normal avec loading state
// ═══════════════════════════════════════════════════════════════

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

/** Cache global en mémoire — partagé entre tous les hooks */
const cache = new Map<string, CacheEntry<unknown>>();

/** Durées TTL par défaut (ms). */
export const CacheTTL = {
  /** Données rarement modifiées (repos, refs). */
  LONG: 5 * 60 * 1000, // 5 minutes
  /** Données de contenu (tree, file, diff). */
  MEDIUM: 2 * 60 * 1000, // 2 minutes
  /** Données fréquemment modifiées (operations list, reviews). */
  SHORT: 30 * 1000, // 30 seconds
  /** Pas de cache. */
  NONE: 0,
} as const;

/**
 * Retourne l'entrée en cache si elle existe et n'a pas expiré.
 * Retourne null si miss ou expiré.
 */
export function getCached<T>(key: string, ttlMs: number): T | null {
  const entry = cache.get(key) as CacheEntry<T> | undefined;
  if (!entry) return null;

  const age = Date.now() - entry.timestamp;
  if (age > ttlMs) {
    // Expiré — on le garde quand même pour le stale-while-revalidate
    // mais on retourne null pour forcer un refetch
    return null;
  }

  return entry.data;
}

/**
 * Retourne l'entrée stale (même expirée) pour un affichage immédiat
 * pendant le refetch en arrière-plan.
 */
export function getStale<T>(key: string): T | null {
  const entry = cache.get(key) as CacheEntry<T> | undefined;
  return entry?.data ?? null;
}

/** Met en cache une donnée avec le timestamp actuel. */
export function setCache<T>(key: string, data: T): void {
  cache.set(key, { data, timestamp: Date.now() });
}

/** Invalide une clé spécifique. */
export function invalidateCache(key: string): void {
  cache.delete(key);
}

/** Invalide toutes les clés qui commencent par le préfixe donné. */
export function invalidateCacheByPrefix(prefix: string): void {
  for (const key of cache.keys()) {
    if (key.startsWith(prefix)) {
      cache.delete(key);
    }
  }
}

/** Vide tout le cache (utile après un push ou une création d'opération). */
export function clearCache(): void {
  cache.clear();
}

/** Retourne le nombre d'entrées en cache (debug). */
export function cacheSize(): number {
  return cache.size;
}
