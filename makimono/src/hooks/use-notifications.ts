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

// ── Paginated Notifications Hook (Page /notifications) ──────

type NotifFilter = "all" | "unread";

interface UseNotificationsPage {
  notifications: Notification[];
  total: number;
  unreadCount: number;
  loading: boolean;
  loadingMore: boolean;
  error: string | null;
  hasMore: boolean;
  filter: NotifFilter;
  setFilter: (f: NotifFilter) => void;
  loadMore: () => void;
  refetch: () => void;
}

const PAGE_SIZE = 20;

/**
 * Paginated notification hook for the dedicated /notifications page.
 * Supports "all" / "unread" client-side filter and "load more" pagination.
 */
export function useNotificationsPage(): UseNotificationsPage {
  const [all, setAll] = useState<Notification[]>([]);
  const [total, setTotal] = useState(0);
  const [unreadCount, setUnreadCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [filter, setFilter] = useState<NotifFilter>("all");
  const mountedRef = useRef(true);

  // Initial fetch
  const fetchPage = useCallback(
    async (pageOffset: number, append: boolean) => {
      if (!append) setLoading(true);
      else setLoadingMore(true);
      setError(null);
      try {
        const result = await fetchNotifications(PAGE_SIZE, pageOffset);
        if (!mountedRef.current) return;
        if (append) {
          setAll((prev) => [...prev, ...result.notifications]);
        } else {
          setAll(result.notifications);
        }
        setTotal(result.total);
        setUnreadCount(result.unread_count);
        setHasMore(pageOffset + PAGE_SIZE < result.total);
      } catch (err) {
        if (mountedRef.current) {
          setError(err instanceof Error ? err.message : "Unknown error");
        }
      } finally {
        if (mountedRef.current) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    []
  );

  useEffect(() => {
    mountedRef.current = true;
    fetchPage(0, false);
    return () => {
      mountedRef.current = false;
    };
  }, [fetchPage]);

  const loadMore = useCallback(() => {
    const nextOffset = offset + PAGE_SIZE;
    setOffset(nextOffset);
    fetchPage(nextOffset, true);
  }, [offset, fetchPage]);

  const refetch = useCallback(() => {
    setOffset(0);
    fetchPage(0, false);
  }, [fetchPage]);

  // Client-side unread filter (applied on already-fetched data)
  const filtered =
    filter === "unread" ? all.filter((n) => !n.read) : all;

  return {
    notifications: filtered,
    total,
    unreadCount,
    loading,
    loadingMore,
    error,
    hasMore: filter === "all" ? hasMore : false, // disable load-more when filtering
    filter,
    setFilter,
    loadMore,
    refetch,
  };
}

// ── Date Grouping Utility ───────────────────────────────────

/** Group notifications by relative date for GitHub-style display. */
export function groupByDate(
  notifications: Notification[]
): { label: string; items: Notification[] }[] {
  const now = new Date();
  const todayStr = now.toDateString();
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  const yesterdayStr = yesterday.toDateString();

  const weekAgo = new Date(now);
  weekAgo.setDate(weekAgo.getDate() - 7);

  const groups: Record<string, Notification[]> = {};
  const order: string[] = [];

  for (const n of notifications) {
    const d = new Date(n.created_at);
    let label: string;
    if (d.toDateString() === todayStr) {
      label = "Aujourd'hui";
    } else if (d.toDateString() === yesterdayStr) {
      label = "Hier";
    } else if (d >= weekAgo) {
      label = "Cette semaine";
    } else {
      label = d.toLocaleDateString("fr-FR", {
        day: "numeric",
        month: "long",
        year: "numeric",
      });
    }
    if (!groups[label]) {
      groups[label] = [];
      order.push(label);
    }
    groups[label].push(n);
  }

  return order.map((label) => ({ label, items: groups[label] }));
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
