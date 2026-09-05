"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono Notification Hooks (Phase 38 — Le Carillon) 🔔
// SWR cache hooks for in-app notifications with polling.
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import {
  type Notification,
  type NotificationsResponse,
  type UnreadCountResponse,
  fetchNotifications,
  fetchUnreadCount,
  markNotificationRead,
  markAllNotificationsRead,
} from "@/lib/notification-api";

// ── Unread Count Hook (polling 30s) ──────────────────────────

interface UseUnreadCount {
  count: number;
  loading: boolean;
  refetch: () => void;
}

/**
 * Polls the unread notification count every 30s.
 * Lightweight endpoint — no payload, just a counter.
 */
export function useUnreadCount(): UseUnreadCount {
  const [count, setCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
    try {
      const result = await fetchUnreadCount();
      if (mountedRef.current) {
        setCount(result.unread_count);
        setLoading(false);
      }
    } catch {
      // Silently fail (user might not be logged in)
      if (mountedRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    execute();
    const interval = setInterval(execute, 30_000); // Poll every 30s
    return () => {
      mountedRef.current = false;
      clearInterval(interval);
    };
  }, [execute]);

  return { count, loading, refetch: execute };
}

// ── Notification List Hook ──────────────────────────────────

interface UseNotifications {
  data: NotificationsResponse | null;
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

/** Fetch the notification list (default: 20 latest, no auto-refresh). */
export function useNotifications(
  limit = 20,
  offset = 0
): UseNotifications {
  const [data, setData] = useState<NotificationsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const mountedRef = useRef(true);

  const execute = useCallback(async () => {
    setError(null);
    try {
      const result = await fetchNotifications(limit, offset);
      if (mountedRef.current) {
        setData(result);
        setLoading(false);
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : "Unknown error");
        setLoading(false);
      }
    }
  }, [limit, offset]);

  useEffect(() => {
    mountedRef.current = true;
    execute();
    return () => {
      mountedRef.current = false;
    };
  }, [execute]);

  return { data, error, loading, refetch: execute };
}

// ── Mutation Helpers ────────────────────────────────────────

/** Mark a notification as read, then refetch. */
export async function handleMarkRead(
  id: string,
  refetchList?: () => void,
  refetchCount?: () => void
) {
  await markNotificationRead(id);
  refetchList?.();
  refetchCount?.();
}

/** Mark all as read, then refetch. */
export async function handleMarkAllRead(
  refetchList?: () => void,
  refetchCount?: () => void
) {
  await markAllNotificationsRead();
  refetchList?.();
  refetchCount?.();
}
