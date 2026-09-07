"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Notifications Page (Phase 38B — Redesign) 🔔
// Full-page notification center, GitHub-style.
// ═══════════════════════════════════════════════════════════════

import { useRouter } from "next/navigation";
import {
  useNotificationsPage,
  groupByDate,
  handleMarkRead,
  handleMarkAllRead,
} from "@/hooks/use-notifications";
import { useUnreadCount } from "@/hooks/use-notifications";
import type { Notification } from "@/lib/notification-api";

import "./notifications-page.css";

// ── Icons ────────────────────────────────────────────────────

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

function getNotifTypeLabel(type: string) {
  switch (type) {
    case "mentioned":
      return "Mention";
    case "review_requested":
      return "Review demandée";
    case "review_received":
      return "Review reçue";
    case "assigned":
      return "Assignation";
    case "issue_closed":
      return "Issue fermée";
    case "mr_merged":
      return "MR mergée";
    default:
      return "Notification";
  }
}

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

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "à l'instant";
  if (minutes < 60) return `il y a ${minutes}min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `il y a ${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `il y a ${days}j`;
  const months = Math.floor(days / 30);
  return `il y a ${months} mois`;
}

// ── Page Component ───────────────────────────────────────────

export default function NotificationsPage() {
  const router = useRouter();
  const {
    notifications,
    total,
    unreadCount,
    loading,
    loadingMore,
    error,
    hasMore,
    filter,
    setFilter,
    loadMore,
    refetch,
  } = useNotificationsPage();

  const { refetch: refetchBadge } = useUnreadCount();

  const groups = groupByDate(notifications);

  async function onMarkRead(id: string) {
    await handleMarkRead(id, refetch, refetchBadge);
  }

  async function onMarkAllRead() {
    await handleMarkAllRead(refetch, refetchBadge);
  }

  function navigateToNotif(n: Notification) {
    if (!n.read) {
      onMarkRead(n.id).catch(() => {});
    }
    router.push(buildNotifUrl(n));
  }

  return (
    <div className="notif-page">
      {/* ── Header ─────────────────────────────────────── */}
      <div className="notif-page-header">
        <div className="notif-page-header-left">
          <h1 className="notif-page-title">
            <span className="notif-page-title-icon">🔔</span>
            Notifications
          </h1>
          {unreadCount > 0 && (
            <span className="notif-page-unread-badge">
              {unreadCount} non lue{unreadCount > 1 ? "s" : ""}
            </span>
          )}
        </div>
        <div className="notif-page-header-right">
          {unreadCount > 0 && (
            <button
              className="notif-page-mark-all-btn"
              onClick={onMarkAllRead}
            >
              <span className="notif-page-mark-all-icon">✓</span>
              Tout marquer comme lu
            </button>
          )}
        </div>
      </div>

      {/* ── Filter Bar ─────────────────────────────────── */}
      <div className="notif-page-filters">
        <button
          className={`notif-filter-btn ${filter === "all" ? "active" : ""}`}
          onClick={() => setFilter("all")}
        >
          <span className="notif-filter-icon">📥</span>
          Toutes
          <span className="notif-filter-count">{total}</span>
        </button>
        <button
          className={`notif-filter-btn ${filter === "unread" ? "active" : ""}`}
          onClick={() => setFilter("unread")}
        >
          <span className="notif-filter-icon">●</span>
          Non lues
          {unreadCount > 0 && (
            <span className="notif-filter-count unread">{unreadCount}</span>
          )}
        </button>
      </div>

      {/* ── Content ────────────────────────────────────── */}
      <div className="notif-page-content">
        {loading && (
          <div className="notif-page-loading">
            <div className="notif-page-spinner" />
            <span>Chargement des notifications…</span>
          </div>
        )}

        {error && (
          <div className="notif-page-error">
            <span>⚠️ Erreur : {error}</span>
            <button className="notif-page-retry-btn" onClick={refetch}>
              Réessayer
            </button>
          </div>
        )}

        {!loading && !error && notifications.length === 0 && (
          <div className="notif-page-empty">
            <span className="notif-page-empty-icon">✨</span>
            <h2 className="notif-page-empty-title">
              {filter === "unread"
                ? "Aucune notification non lue"
                : "Aucune notification"}
            </h2>
            <p className="notif-page-empty-desc">
              {filter === "unread"
                ? "Vous êtes à jour ! Toutes les notifications ont été lues."
                : "Les mentions, reviews et assignations apparaîtront ici."}
            </p>
            {filter === "unread" && (
              <button
                className="notif-page-show-all-btn"
                onClick={() => setFilter("all")}
              >
                Voir toutes les notifications
              </button>
            )}
          </div>
        )}

        {!loading &&
          !error &&
          groups.map((group) => (
            <div key={group.label} className="notif-page-group">
              <div className="notif-page-group-header">
                <span className="notif-page-group-label">{group.label}</span>
              </div>
              <div className="notif-page-group-list">
                {group.items.map((n) => (
                  <div
                    key={n.id}
                    role="link"
                    tabIndex={0}
                    className={`notif-page-item ${n.read ? "read" : "unread"}`}
                    onClick={() => navigateToNotif(n)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") navigateToNotif(n);
                    }}
                  >
                    {/* Unread indicator bar */}
                    {!n.read && <div className="notif-page-item-bar" />}

                    {/* Icon */}
                    <span className="notif-page-item-icon">
                      {getNotifIcon(n.notification_type)}
                    </span>

                    {/* Content */}
                    <div className="notif-page-item-body">
                      <div className="notif-page-item-message">
                        {n.message}
                      </div>
                      <div className="notif-page-item-meta">
                        <span className="notif-page-item-repo">
                          {n.repository_owner}/{n.repository_name}
                        </span>
                        <span className="notif-page-item-sep">·</span>
                        <span className="notif-page-item-type">
                          {getNotifTypeLabel(n.notification_type)}
                        </span>
                        <span className="notif-page-item-sep">·</span>
                        <span className="notif-page-item-time">
                          {timeAgo(n.created_at)}
                        </span>
                      </div>
                    </div>

                    {/* Actions */}
                    <div className="notif-page-item-actions">
                      {!n.read && (
                        <button
                          className="notif-page-item-read-btn"
                          title="Marquer comme lu"
                          onClick={(e) => {
                            e.stopPropagation();
                            onMarkRead(n.id);
                          }}
                        >
                          ✓
                        </button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}

        {/* ── Load More ────────────────────────────────── */}
        {hasMore && !loading && (
          <div className="notif-page-load-more">
            <button
              className="notif-page-load-more-btn"
              onClick={loadMore}
              disabled={loadingMore}
            >
              {loadingMore ? (
                <>
                  <span className="notif-page-spinner small" />
                  Chargement…
                </>
              ) : (
                "Charger plus de notifications"
              )}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
