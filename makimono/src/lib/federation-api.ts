// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Federation API Client (Phase 31)
// ActivityPub-aware fetch wrappers for Outbox, Inbox, Followers
// ═══════════════════════════════════════════════════════════════

import { getBaseUrl } from "./api";

// ── Types ────────────────────────────────────────────────────

/** An ActivityPub OrderedCollection (outbox, followers). */
export interface APOrderedCollection {
  "@context": string;
  id: string;
  type: "OrderedCollection";
  totalItems: number;
  orderedItems: APActivity[] | string[];
}

/** An ActivityPub activity from the outbox. */
export interface APActivity {
  "@context"?: string | string[];
  id?: string;
  type: string;
  actor?: string;
  object?: APObject | string;
  published?: string;
  [key: string]: unknown;
}

/** An ActivityPub object (Repository, Note, etc.). */
export interface APObject {
  type?: string;
  id?: string;
  name?: string;
  summary?: string;
  attributedTo?: string;
  [key: string]: unknown;
}

/** An inbox activity from the private REST endpoint. */
export interface InboxActivityItem {
  id: string;
  remoteActorUri: string;
  activityType: string;
  objectType: string;
  objectUri: string;
  processed: boolean;
  receivedAt: string;
  processedAt: string | null;
}

/** Response from GET /api/v1/actors/{handle}/inbox/activities. */
export interface InboxActivitiesResponse {
  totalItems: number;
  items: InboxActivityItem[];
}

/** NodeInfo 2.1 response. */
export interface NodeInfoResponse {
  version: string;
  software: {
    name: string;
    version: string;
    repository?: string;
    homepage?: string;
  };
  protocols: string[];
  openRegistrations: boolean;
  usage: {
    users: { total: number; activeMonth: number; activeHalfyear: number };
    localPosts: number;
  };
  metadata: {
    nodeDescription?: string;
    features?: string[];
  };
}

// ── Fetch Helpers ────────────────────────────────────────────

/**
 * Fetch for ActivityPub endpoints.
 *
 * ## Piège du Header Accept
 * Le Fediverse exige `Accept: application/activity+json` — sans ce header,
 * le backend retourne `406 Not Acceptable`. Mastodon, Forgejo, et notre
 * propre Taijutsu vérifient tous ce header.
 */
async function apFetch<T>(path: string): Promise<T> {
  const url = `${getBaseUrl()}${path}`;
  const res = await fetch(url, {
    headers: {
      Accept: "application/activity+json",
    },
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`AP fetch failed (${res.status}): ${text}`);
  }

  return res.json() as Promise<T>;
}

/**
 * Fetch for private REST endpoints requiring JWT.
 */
async function authFetch<T>(path: string, token: string): Promise<T> {
  const url = `${getBaseUrl()}${path}`;
  const res = await fetch(url, {
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "Unknown error");
    throw new Error(`Auth fetch failed (${res.status}): ${text}`);
  }

  return res.json() as Promise<T>;
}

// ── API Functions ────────────────────────────────────────────

/**
 * Outbox ActivityPub — `GET /actors/{handle}/outbox`
 *
 * Public. Returns an OrderedCollection of published activities.
 */
export async function getFederationOutbox(
  handle: string,
): Promise<APOrderedCollection> {
  return apFetch<APOrderedCollection>(
    `/actors/${encodeURIComponent(handle)}/outbox`,
  );
}

/**
 * Followers ActivityPub — `GET /actors/{handle}/followers`
 *
 * Public. Returns an OrderedCollection of follower URIs.
 */
export async function getFederationFollowers(
  handle: string,
): Promise<APOrderedCollection> {
  return apFetch<APOrderedCollection>(
    `/actors/${encodeURIComponent(handle)}/followers`,
  );
}

/**
 * Inbox Activities (Private) — `GET /api/v1/actors/{handle}/inbox/activities`
 *
 * 🔒 JWT required. Returns paginated inbox activities.
 */
export async function getInboxActivities(
  handle: string,
  token: string,
  limit = 50,
): Promise<InboxActivitiesResponse> {
  return authFetch<InboxActivitiesResponse>(
    `/api/v1/actors/${encodeURIComponent(handle)}/inbox/activities?limit=${limit}`,
    token,
  );
}

/**
 * NodeInfo 2.1 — `GET /nodeinfo/2.1`
 *
 * Public. Instance metadata (software, protocols, usage stats).
 */
export async function getNodeInfo(): Promise<NodeInfoResponse> {
  const url = `${getBaseUrl()}/nodeinfo/2.1`;
  const res = await fetch(url, {
    headers: { Accept: "application/json" },
  });

  if (!res.ok) {
    throw new Error(`NodeInfo fetch failed (${res.status})`);
  }

  return res.json() as Promise<NodeInfoResponse>;
}

// ── Vegapunk Semantic Parser ─────────────────────────────────
// Translate raw ActivityPub JSON-LD into human-readable descriptions.

/** Parsed activity for display. */
export interface ParsedActivity {
  icon: string;
  verb: string;
  actorHandle: string;
  objectName: string;
  objectType: string;
  timestamp: string;
  color: string;
}

/** Map activity type to icon + color + verb. */
const ACTIVITY_SEMANTICS: Record<
  string,
  { icon: string; verb: string; color: string }
> = {
  Create: { icon: "🏗️", verb: "a créé", color: "var(--fed-color-create)" },
  Push: { icon: "📤", verb: "a poussé vers", color: "var(--fed-color-push)" },
  Accept: { icon: "✅", verb: "a accepté", color: "var(--fed-color-accept)" },
  Follow: { icon: "🤝", verb: "suit", color: "var(--fed-color-follow)" },
  Update: {
    icon: "✏️",
    verb: "a mis à jour",
    color: "var(--fed-color-update)",
  },
  Delete: { icon: "🗑️", verb: "a supprimé", color: "var(--fed-color-delete)" },
  Announce: {
    icon: "📢",
    verb: "a partagé",
    color: "var(--fed-color-announce)",
  },
  Undo: { icon: "↩️", verb: "a annulé", color: "var(--fed-color-undo)" },
};

/** Extract a human-readable handle from an actor URI. */
function extractHandle(uri: string): string {
  if (!uri) return "inconnu";
  // Try to extract the last segment of the URI
  const segments = uri.replace(/\/$/, "").split("/");
  return `@${segments[segments.length - 1]}`;
}

/** Extract the object name from an AP activity. */
function extractObjectName(activity: APActivity): string {
  if (!activity.object) return "";
  if (typeof activity.object === "string") {
    // URI-only object (Announce style)
    const segments = activity.object.replace(/\/$/, "").split("/");
    return segments.slice(-2).join("/");
  }
  return (
    activity.object.name ||
    activity.object.type ||
    ""
  );
}

/** Extract the object type from an AP activity. */
function extractObjectType(activity: APActivity): string {
  if (!activity.object) return "";
  if (typeof activity.object === "string") return "URI";
  return activity.object.type || "";
}

/**
 * Parse a raw AP activity into a human-readable form.
 * The "Vegapunk Tweak" — humans need stories, not JSON-LD.
 */
export function parseOutboxActivity(activity: APActivity): ParsedActivity {
  const type = activity.type || "Unknown";
  const semantics = ACTIVITY_SEMANTICS[type] || {
    icon: "📋",
    verb: "a effectué",
    color: "var(--muted-foreground)",
  };

  return {
    icon: semantics.icon,
    verb: semantics.verb,
    actorHandle: extractHandle(
      (activity.actor as string) || "",
    ),
    objectName: extractObjectName(activity),
    objectType: extractObjectType(activity),
    timestamp: (activity.published as string) || "",
    color: semantics.color,
  };
}

/**
 * Parse an inbox activity item into a human-readable form.
 */
export function parseInboxActivity(item: InboxActivityItem): ParsedActivity {
  const type = item.activityType || "Unknown";
  const semantics = ACTIVITY_SEMANTICS[type] || {
    icon: "📋",
    verb: "a effectué",
    color: "var(--muted-foreground)",
  };

  return {
    icon: semantics.icon,
    verb: semantics.verb,
    actorHandle: extractHandle(item.remoteActorUri),
    objectName: item.objectUri
      ? item.objectUri.replace(/\/$/, "").split("/").slice(-2).join("/")
      : item.objectType,
    objectType: item.objectType,
    timestamp: item.receivedAt,
    color: semantics.color,
  };
}
