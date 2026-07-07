// ═══════════════════════════════════════════════════════════════
// SHINOBI — Explorer API (Phase 6)
// Types and fetch helpers for the Code Explorer routes
// ═══════════════════════════════════════════════════════════════

import { buildRepoPrefix } from "./api";

// ── Types ────────────────────────────────────────────────────

export interface TreeEntryItem {
  name: string;
  path: string;
  kind: "file" | "directory";
  size: number | null;
}

/** Response discriminante de l'API /tree/{revision} */
export type ExplorerResponse =
  | {
      kind: "directory";
      revision: string;
      path: string;
      entries: TreeEntryItem[];
      count: number;
    }
  | {
      kind: "file";
      revision: string;
      path: string;
      size: number;
      is_text: boolean;
      language: string | null;
      content_b64: string;
    };

export interface RefItem {
  name: string;
  target: string;
}

export interface RefsResponse {
  branches: RefItem[];
  tags: RefItem[];
  total: number;
}

// ── Decode base64 → string (UTF-8) ───────────────────────────

export function decodeBase64(b64: string): string {
  try {
    return decodeURIComponent(
      atob(b64)
        .split("")
        .map((c) => "%" + c.charCodeAt(0).toString(16).padStart(2, "0"))
        .join("")
    );
  } catch {
    // Fallback pour les binaires qui ne sont pas du UTF-8 valide
    return atob(b64);
  }
}

// ── API Functions ────────────────────────────────────────────

/**
 * GET /api/v1/repos/{owner}/{repo}/tree/{revision}?path={path}
 *
 * Retourne soit une liste de répertoire (kind="directory") soit
 * le contenu d'un fichier (kind="file") — un seul appel suffit.
 */
export async function exploreTree(
  owner: string,
  repo: string,
  revision: string,
  path: string = ""
): Promise<ExplorerResponse> {
  const prefix = buildRepoPrefix(owner, repo);
  const pathParam = path ? `?path=${encodeURIComponent(path)}` : "";
  const url = `${prefix}/tree/${encodeURIComponent(revision)}${pathParam}`;

  const res = await fetch(url);
  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`Explorer API error ${res.status}: ${text}`);
  }
  return res.json() as Promise<ExplorerResponse>;
}

/**
 * GET /api/v1/repos/{owner}/{repo}/refs
 */
export async function listRefs(
  owner: string,
  repo: string
): Promise<RefsResponse> {
  const prefix = buildRepoPrefix(owner, repo);
  const res = await fetch(`${prefix}/refs`);
  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`Refs API error ${res.status}: ${text}`);
  }
  return res.json() as Promise<RefsResponse>;
}

// ── Helpers ──────────────────────────────────────────────────

/** Construit l'URL de navigation vers un chemin dans le dépôt. */
export function buildTreeUrl(
  owner: string,
  repo: string,
  revision: string,
  path: string = ""
): string {
  const segments = [owner, repo, "tree", revision, ...path.split("/").filter(Boolean)];
  return "/" + segments.map(encodeURIComponent).join("/");
}

/** Construit les segments du breadcrumb depuis un chemin complet. */
export function buildBreadcrumbs(
  owner: string,
  repo: string,
  revision: string,
  path: string
): Array<{ label: string; href: string }> {
  const crumbs: Array<{ label: string; href: string }> = [
    { label: owner, href: `/${owner}` },
    { label: repo, href: `/${owner}/${repo}` },
    { label: revision, href: buildTreeUrl(owner, repo, revision) },
  ];

  if (!path) return crumbs;

  const parts = path.split("/").filter(Boolean);
  let accumulated = "";
  for (const part of parts) {
    accumulated = accumulated ? `${accumulated}/${part}` : part;
    crumbs.push({
      label: part,
      href: buildTreeUrl(owner, repo, revision, accumulated),
    });
  }

  return crumbs;
}
