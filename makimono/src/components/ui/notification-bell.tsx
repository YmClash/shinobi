"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — NotificationBell (Phase 38B — Redesign) 🔔
// Mini-dropdown (5 dernières) + lien vers /notifications.
// Uses @floating-ui for rock-solid positioning.
// ═══════════════════════════════════════════════════════════════

import { useState, useRef, useEffect, useCallback } from "react";
import { useRouter } from "next/navigation";
import {
  useFloating,
  offset,
  flip,
  shift,
  autoUpdate,
} from "@floating-ui/react-dom";
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
  } = useNotifications(5); // Max 5 in mini-dropdown

  // ── @floating-ui positioning ─────────────────────────────
  const { refs, floatingStyles } = useFloating({
    strategy: "fixed",
    placement: "right-start",
    middleware: [
      offset(8),
      flip({ fallbackPlacements: ["right-end", "top-start", "bottom-start"] }),
      shift({ padding: 10 }),
    ],
    whileElementsMounted: autoUpdate,
  });

  // ── Close on outside click ────────────────────────────────
  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      const target = e.target as Node;
      if (
        panelRef.current && !panelRef.current.contains(target) &&
        refs.reference.current && !(refs.reference.current as HTMLElement).contains(target)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open, refs.reference]);

  // ── Close on Escape key ───────────────────────────────────
  useEffect(() => {
    if (!open) return;
    function handleEscape(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [open]);

  // ── Handlers ──────────────────────────────────────────────

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

  function onViewAll() {
    setOpen(false);
    router.push("/notifications");
  }

  const notifications = notifData?.notifications ?? [];

  return (
    <div className="notif-bell-container">
      {/* Bell Button */}
      <button
        ref={refs.setReference}
        className={`notif-bell-btn ${collapsed ? "collapsed" : ""}`}
        onClick={handleToggle}
        title="Notifications"
        aria-label="Notifications"
        aria-haspopup="true"
        aria-expanded={open}
      >
        <span className="notif-bell-icon-wrap">
          <span className="notif-bell-icon">🔔</span>
          {count > 0 && (
            <span className="notif-badge">
              {count > 99 ? "99+" : count}
            </span>
          )}
        </span>
        {!collapsed && <span className="notif-bell-label">Notifications</span>}
      </button>

      {/* Mini-Dropdown Panel — @floating-ui positioned */}
      {open && (
        <>
          {/* Invisible backdrop for click-outside */}
          <div
            className="notif-backdrop"
            onClick={() => setOpen(false)}
            aria-hidden="true"
          />
          <div
            ref={(node) => {
              panelRef.current = node;
              refs.setFloating(node);
            }}
            className="notif-panel"
            style={floatingStyles}
            role="menu"
            aria-label="Notifications récentes"
          >
            {/* Header */}
            <div className="notif-panel-header">
              <span className="notif-panel-title">🔔 Notifications</span>
              {count > 0 && (
                <button
                  className="notif-mark-all-btn"
                  onClick={onMarkAllRead}
                >
                  Tout lire
                </button>
              )}
            </div>

            {/* List (max 5) */}
            <div className="notif-panel-list">
              {notifications.length === 0 && (
                <div className="notif-empty">
                  Aucune notification ✨
                </div>
              )}

              {notifications.map((n) => (
                <div
                  key={n.id}
                  role="menuitem"
                  tabIndex={0}
                  className={`notif-item ${n.read ? "read" : "unread"}`}
                  onClick={async () => {
                    if (!n.read) {
                      await onMarkRead(n.id).catch(() => { });
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

            {/* Footer — View All link */}
            <div className="notif-panel-footer">
              <button
                className="notif-view-all-btn"
                onClick={onViewAll}
              >
                Voir toutes les notifications →
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
