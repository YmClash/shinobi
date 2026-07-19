// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Auth Client (Phase 19A)
// JWT token management + API calls for register/login/me/tokens
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl } from "./api";

// ── Types ────────────────────────────────────────────────────

export interface AuthActor {
  id: string;
  handle: string;
  display_name: string;
  actor_type: string;
  email: string | null;
  avatar_url: string | null;
  bio?: string | null;
  github_id?: number | null;
  created_at: string;
}

export interface AuthResponse {
  token: string;
  actor: AuthActor;
}

export interface PatToken {
  token: string;
  label: string | null;
  warning?: string;
}

export interface PatInfo {
  id: string;
  label: string | null;
  created_at: string;
}

export interface PatListResponse {
  tokens: PatInfo[];
  count: number;
}

// ── Token Storage ────────────────────────────────────────────

const TOKEN_KEY = "shinobi_jwt";
const USER_KEY = "shinobi_user";

/** Persiste le JWT dans localStorage. */
export function setToken(token: string): void {
  if (typeof window !== "undefined") {
    localStorage.setItem(TOKEN_KEY, token);
  }
}

/** Récupère le JWT depuis localStorage. */
export function getToken(): string | null {
  if (typeof window === "undefined") return null;
  return localStorage.getItem(TOKEN_KEY);
}

/** Supprime le JWT. */
export function clearToken(): void {
  if (typeof window !== "undefined") {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USER_KEY);
  }
}

/** Persiste l'acteur dans localStorage (cache UX). */
export function setStoredUser(actor: AuthActor): void {
  if (typeof window !== "undefined") {
    localStorage.setItem(USER_KEY, JSON.stringify(actor));
  }
}

/** Récupère l'acteur caché pour hydratation instantanée. */
export function getStoredUser(): AuthActor | null {
  if (typeof window === "undefined") return null;
  const raw = localStorage.getItem(USER_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as AuthActor;
  } catch {
    return null;
  }
}

// ── Auth Headers ────────────────────────────────────────────

/** Retourne les headers avec le JWT si disponible. */
export function authHeaders(): HeadersInit {
  const token = getToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }
  return headers;
}

// ── API Calls ───────────────────────────────────────────────

const base = () => getBaseUrl();

/** POST /api/v1/auth/register — Inscription. */
export async function register(data: {
  handle: string;
  display_name: string;
  email: string;
  password: string;
}): Promise<AuthResponse> {
  const res = await fetch(`${base()}/api/v1/auth/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error?.message ?? `Erreur ${res.status}`);
  }
  return res.json();
}

/** POST /api/v1/auth/login — Connexion. */
export async function login(data: {
  email: string;
  password: string;
}): Promise<AuthResponse> {
  const res = await fetch(`${base()}/api/v1/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error?.message ?? `Identifiants invalides`);
  }
  return res.json();
}

/** GET /api/v1/auth/me — Profil de l'acteur authentifié. */
export async function fetchMe(): Promise<AuthActor> {
  const res = await fetch(`${base()}/api/v1/auth/me`, {
    headers: authHeaders(),
  });
  if (!res.ok) {
    throw new Error("Session expirée");
  }
  return res.json();
}

/** POST /api/v1/auth/tokens — Créer un PAT. */
export async function createPat(label?: string): Promise<PatToken> {
  const res = await fetch(`${base()}/api/v1/auth/tokens`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ label: label || null }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error?.message ?? `Erreur ${res.status}`);
  }
  return res.json();
}

/** GET /api/v1/auth/tokens — Lister les PAT. */
export async function listPats(): Promise<PatListResponse> {
  const res = await fetch(`${base()}/api/v1/auth/tokens`, {
    headers: authHeaders(),
  });
  if (!res.ok) {
    throw new Error("Impossible de charger les tokens");
  }
  return res.json();
}

// ── GitHub OAuth (Phase 20) ─────────────────────────────────

export interface GitHubOAuthResponse extends AuthResponse {
  is_new_account: boolean;
}

/** GET /api/v1/auth/github — Récupère l'URL d'autorisation GitHub (avec state anti-CSRF). */
export async function getGitHubAuthUrl(): Promise<string> {
  const res = await fetch(`${base()}/api/v1/auth/github`);
  if (!res.ok) {
    throw new Error("GitHub OAuth non disponible");
  }
  const data = await res.json();
  return data.url;
}

/** POST /api/v1/auth/github/callback — Échange le code OAuth contre un JWT SHINOBI. */
export async function exchangeGitHubCode(
  code: string,
  state: string
): Promise<GitHubOAuthResponse> {
  const res = await fetch(`${base()}/api/v1/auth/github/callback`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, state }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error?.message ?? `Erreur GitHub OAuth ${res.status}`);
  }
  return res.json();
}

// ── Phase 20B — Le Clonage Massif ─────────────────────────

export interface GitHubRepoWithStatus {
  full_name: string;
  name: string;
  description: string | null;
  clone_url: string;
  default_branch: string;
  stars: number;
  forks: number;
  language: string | null;
  license: string | null;
  is_private: boolean;
  already_imported: boolean;
}

export interface GitHubReposResponse {
  repos: GitHubRepoWithStatus[];
  count: number;
}

export interface BulkImportItemStatus {
  github_url: string;
  shinobi_name: string | null;
  status: "imported" | "skipped" | "error";
  message: string | null;
}

export interface BulkImportResult {
  results: BulkImportItemStatus[];
  imported: number;
  skipped: number;
  failed: number;
  total: number;
}

/** GET /api/v1/github/my-repos — Liste les repos GitHub de l'utilisateur connecté. */
export async function fetchGitHubRepos(): Promise<GitHubReposResponse> {
  const res = await fetch(`${base()}/api/v1/github/my-repos`, {
    headers: authHeaders(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error?.message ?? `Erreur ${res.status}`);
  }
  return res.json();
}

/** POST /api/v1/github/bulk-import — Import massif de repos GitHub. */
export async function bulkImportGitHub(
  repoUrls: string[]
): Promise<BulkImportResult> {
  const res = await fetch(`${base()}/api/v1/github/bulk-import`, {
    method: "POST",
    headers: authHeaders(),
    body: JSON.stringify({ repo_urls: repoUrls }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err?.error?.message ?? `Erreur ${res.status}`);
  }
  return res.json();
}
