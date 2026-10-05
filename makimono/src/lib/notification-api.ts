// ── Notification API Client — Phase 38 (Le Carillon) 🔔 ──────────────
// Client API typé pour les notifications in-app.

import { getBaseUrl } from "./api";
import { authHeaders } from "./auth";

const base = () => `${getBaseUrl()}/api/v1`;

// ── Types ────────────────────────────────────────────────────────────

export interface Notification {
  id: string;
  actor_id: string;
  actor_handle?: string;
  notification_type:
    | "mentioned"
    | "review_requested"
    | "review_received"
    | "assigned"
    | "issue_closed"
    | "mr_merged";
  target_type: "issue" | "merge_request" | "comment";
  target_id: string;
  target_number: number | null;
  repository_owner: string;
  repository_name: string;
  message: string;
  read: boolean;
  read_at: string | null;
  created_at: string;
}

export interface NotificationsResponse {
  notifications: Notification[];
  total: number;
  unread_count: number;
}

export interface UnreadCountResponse {
  unread_count: number;
}

// ── API Functions ────────────────────────────────────────────────────

/** Fetch paginated notifications for the authenticated user. */
export async function fetchNotifications(
  limit = 20,
  offset = 0
): Promise<NotificationsResponse> {
  const res = await fetch(
    `${base()}/notifications?limit=${limit}&offset=${offset}`,
    { headers: authHeaders() }
  );
  if (!res.ok) throw new Error(`Notifications: ${res.status}`);
  return res.json();
}

/** Fetch only the unread count (lightweight — used for badge polling). */
export async function fetchUnreadCount(): Promise<UnreadCountResponse> {
  const res = await fetch(`${base()}/notifications/unread-count`, {
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(`Unread count: ${res.status}`);
  return res.json();
}

/** Mark a single notification as read. */
export async function markNotificationRead(id: string): Promise<void> {
  const res = await fetch(`${base()}/notifications/${id}/read`, {
    method: "PATCH",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(`Mark read: ${res.status}`);
}

/** Mark all notifications as read. */
export async function markAllNotificationsRead(): Promise<{ updated: number }> {
  const res = await fetch(`${base()}/notifications/read-all`, {
    method: "PATCH",
    headers: authHeaders(),
  });
  if (!res.ok) throw new Error(`Mark all read: ${res.status}`);
  return res.json();
}
