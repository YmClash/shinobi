"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — NotificationBell (Phase 38 — Le Carillon) 🔔
// Bell icon with unread badge + dropdown panel.
// ═══════════════════════════════════════════════════════════════

import { useState, useRef, useEffect } from "react";
import { useRouter } from "next/navigation";
import {
  useUnreadCount,
  useNotifications,
  handleMarkRead,
  handleMarkAllRead,
} from "@/hooks/use-notifications";
import type { Notification } from "@/lib/notification-api";

import "./notifications.css";

// ── Notification Type Icons ─────────────────────────────────

function getNotifIcon(type: string) {
  switch (type) {
    case "mentioned":
      return "📣";
    case "review_requested":
      return "🔍";
    case "review_received":
      return "✅";
    case "assigned":
      return "👤";
    case "issue_closed":
      return "🔒";
    case "mr_merged":
      return "⚔️";
    default:
      return "🔔";
  }
}

/** Build the navigation URL from a notification. */
function buildNotifUrl(n: Notification): string {
  const base = `/${n.repository_owner}/${n.repository_name}`;
  if (n.target_type === "issue" && n.target_number != null) {
    return `${base}/issues/${n.target_number}`;
  }
  if (n.target_type === "merge_request" && n.target_number != null) {
    return `${base}/mrs/${n.target_number}`;
  }
  return base;
}

/** Format relative time. */
function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}

// ── NotificationBell Component ──────────────────────────────

interface NotificationBellProps {
  collapsed?: boolean;
}

export function NotificationBell({ collapsed = false }: NotificationBellProps) {
  const [open, setOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const router = useRouter();
  const { count, refetch: refetchCount } = useUnreadCount();
  const {
    data: notifData,
    refetch: refetchList,
  } = useNotifications(10);

  // Close on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  // Refresh list when opening panel
  function handleToggle() {
    if (!open) refetchList();
    setOpen(!open);
  }

  async function onMarkRead(id: string) {
    await handleMarkRead(id, refetchList, refetchCount);
  }

  async function onMarkAllRead() {
    await handleMarkAllRead(refetchList, refetchCount);
    setOpen(false);
  }

  const notifications = notifData?.notifications ?? [];

  return (
    <div className="notif-bell-container" ref={panelRef}>
      {/* Bell Button */}
      <button
        className={`notif-bell-btn ${collapsed ? "collapsed" : ""}`}
        onClick={handleToggle}
        title="Notifications"
        aria-label="Notifications"
      >
        <span className="notif-bell-icon">🔔</span>
        {count > 0 && (
          <span className="notif-badge">
            {count > 99 ? "99+" : count}
          </span>
        )}
        {!collapsed && <span className="notif-bell-label">Notifications</span>}
      </button>

      {/* Dropdown Panel */}
      {open && (
        <div className="notif-panel">
          <div className="notif-panel-header">
            <span className="notif-panel-title">🔔 Notifications</span>
            {count > 0 && (
              <button
                className="notif-mark-all-btn"
                onClick={onMarkAllRead}
              >
                Mark all read
              </button>
            )}
          </div>

          <div className="notif-panel-list">
            {notifications.length === 0 && (
              <div className="notif-empty">
                No notifications yet ✨
              </div>
            )}

            {notifications.map((n) => (
              <div
                key={n.id}
                role="link"
                tabIndex={0}
                className={`notif-item ${n.read ? "read" : "unread"}`}
                onClick={async () => {
                  // Garde-fou 404 : markAsRead AVANT la navigation
                  // Si l'issue/MR a été supprimée, Next.js affichera sa page 404
                  // mais la notification sera déjà nettoyée du cache.
                  if (!n.read) {
                    await onMarkRead(n.id).catch(() => {});
                  }
                  setOpen(false);
                  router.push(buildNotifUrl(n));
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.currentTarget.click();
                  }
                }}
              >
                <span className="notif-item-icon">{getNotifIcon(n.notification_type)}</span>
                <div className="notif-item-content">
                  <span className="notif-item-message">{n.message}</span>
                  <span className="notif-item-meta">
                    {n.repository_owner}/{n.repository_name} · {timeAgo(n.created_at)}
                  </span>
                </div>
                {!n.read && <span className="notif-item-dot" />}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
