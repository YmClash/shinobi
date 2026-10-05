"use client";

// ═══════════════════════════════════════════════════════════════
// Phase 31 — Federation Dashboard (Makimono)
// Mission Control for ActivityPub/ForgeFed monitoring
//
// Architecture:
//   Stats Overview → 4 KPI Cards
//   Tabs → Outbox (public) | Inbox (JWT) | Followers (public)
//
// Anti Hydration Mismatch:
//   All relative timestamps rendered client-only via <RelativeTime>
// ═══════════════════════════════════════════════════════════════

import { useEffect, useState } from "react";
import Link from "next/link";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuth } from "@/hooks/use-auth";
import {
  useFederationOutbox,
  useFederationFollowers,
  useInboxActivities,
  useNodeInfo,
  useFederationStats,
} from "@/hooks/use-federation";
import {
  parseOutboxActivity,
  parseInboxActivity,
  type APActivity,
  type InboxActivityItem,
  type ParsedActivity,
} from "@/lib/federation-api";
import "@/styles/federation.css";

// ── Anti Hydration Mismatch: Client-Only Relative Timestamps ──

function RelativeTime({ iso }: { iso: string }) {
  const [text, setText] = useState<string>("");

  useEffect(() => {
    function computeRelative() {
      if (!iso) {
        setText("");
        return;
      }
      const then = new Date(iso).getTime();
      const now = Date.now();
      const diffMs = now - then;
      const diffSec = Math.floor(diffMs / 1000);

      if (diffSec < 60) {
        setText("à l'instant");
      } else if (diffSec < 3600) {
        const m = Math.floor(diffSec / 60);
        setText(`il y a ${m} min`);
      } else if (diffSec < 86400) {
        const h = Math.floor(diffSec / 3600);
        setText(`il y a ${h}h`);
      } else if (diffSec < 604800) {
        const d = Math.floor(diffSec / 86400);
        setText(`il y a ${d}j`);
      } else {
        setText(
          new Date(iso).toLocaleDateString("fr-FR", {
            day: "numeric",
            month: "short",
          }),
        );
      }
    }

    computeRelative();
    const interval = setInterval(computeRelative, 30_000);
    return () => clearInterval(interval);
  }, [iso]);

  return <span title={iso}>{text}</span>;
}

// ── Tab Types ────────────────────────────────────────────────

type TabId = "outbox" | "inbox" | "followers";

const TABS: { id: TabId; icon: string; label: string }[] = [
  { id: "outbox", icon: "📤", label: "Outbox" },
  { id: "inbox", icon: "📥", label: "Inbox" },
  { id: "followers", icon: "👥", label: "Followers" },
];

// ═══════════════════════════════════════════════════════════════
// Main Page Component
// ═══════════════════════════════════════════════════════════════

export default function FederationPage() {
  const { user } = useAuth();
  const handle = user?.handle ?? null;
  const [activeTab, setActiveTab] = useState<TabId>("outbox");

  // Hooks
  const { stats, loading: statsLoading } = useFederationStats(handle);
  const { data: outbox, loading: outLoading } = useFederationOutbox(handle);
  const { data: inbox, loading: inLoading } = useInboxActivities(handle);
  const { data: followers, loading: folLoading } =
    useFederationFollowers(handle);
  const { data: nodeInfo } = useNodeInfo();

  return (
    <div className="fed-page">
      {/* ── Header ─────────────────────────────────────── */}
      <div className="fed-page-header">
        <h1>🌐 Fédération</h1>
        <span className="fed-page-header-badge">
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: "currentColor",
              animation: "fed-pulse 2s ease-in-out infinite",
            }}
          />
          ActivityPub
        </span>
      </div>

      {/* ── Stats Overview — 4 KPI Cards ───────────────── */}
      <div className="fed-stats-grid">
        <StatCard
          icon="📤"
          value={stats.outboxCount}
          label="Outbox"
          sub="Activités publiées"
          accentColor="var(--fed-color-push)"
          loading={statsLoading}
          delay={1}
        />
        <StatCard
          icon="📥"
          value={stats.inboxCount}
          label="Inbox"
          sub="Activités reçues"
          accentColor="var(--fed-color-follow)"
          loading={statsLoading}
          delay={2}
        />
        <StatCard
          icon="👥"
          value={stats.followersCount}
          label="Followers"
          sub="Instances fédérées"
          accentColor="var(--fed-color-accept)"
          loading={statsLoading}
          delay={3}
        />
        <StatCard
          icon="🔗"
          value={nodeInfo?.software?.version ?? "—"}
          label="Instance"
          loading={statsLoading}
          delay={4}
        >
          {nodeInfo && (
            <div className="fed-instance-info">
              {nodeInfo.protocols.map((p) => (
                <span key={p} className="fed-instance-chip">
                  {p}
                </span>
              ))}
              {nodeInfo.metadata.features?.slice(0, 3).map((f) => (
                <span key={f} className="fed-instance-chip">
                  {f}
                </span>
              ))}
            </div>
          )}
        </StatCard>
      </div>

      <Separator />

      {/* ── Tabs ───────────────────────────────────────── */}
      <div className="fed-tabs" role="tablist">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            role="tab"
            aria-selected={activeTab === tab.id}
            data-active={activeTab === tab.id}
            className="fed-tab"
            onClick={() => setActiveTab(tab.id)}
            id={`fed-tab-${tab.id}`}
          >
            <span>{tab.icon}</span>
            <span>{tab.label}</span>
            <span className="fed-tab-count">
              {tab.id === "outbox" && (stats.outboxCount ?? 0)}
              {tab.id === "inbox" && (stats.inboxCount ?? 0)}
              {tab.id === "followers" && (stats.followersCount ?? 0)}
            </span>
          </button>
        ))}
      </div>

      {/* ── Tab Content ────────────────────────────────── */}
      <div className="fed-tab-content" role="tabpanel">
        {activeTab === "outbox" && (
          <OutboxTab
            activities={
              (outbox?.orderedItems as APActivity[] | undefined) ?? []
            }
            loading={outLoading}
          />
        )}
        {activeTab === "inbox" && (
          <InboxTab
            activities={inbox?.items ?? []}
            loading={inLoading}
            isAuthenticated={!!user}
          />
        )}
        {activeTab === "followers" && (
          <FollowersTab
            followers={(followers?.orderedItems as string[]) ?? []}
            loading={folLoading}
          />
        )}
      </div>
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════
// Sub-components
// ═══════════════════════════════════════════════════════════════

// ── Stat Card ────────────────────────────────────────────────

function StatCard({
  icon,
  value,
  label,
  sub,
  accentColor,
  loading,
  delay,
  children,
}: {
  icon: string;
  value: number | string;
  label: string;
  sub?: string;
  accentColor?: string;
  loading: boolean;
  delay: number;
  children?: React.ReactNode;
}) {
  if (loading) {
    return (
      <div
        className={`fed-stat-card fed-animate-in fed-stagger-${delay}`}
        style={{ "--stat-accent": accentColor } as React.CSSProperties}
      >
        <span className="fed-stat-icon">{icon}</span>
        <Skeleton className="h-8 w-16 rounded" />
        <Skeleton className="h-3 w-20 rounded mt-2" />
      </div>
    );
  }

  return (
    <div
      className={`fed-stat-card fed-animate-in fed-stagger-${delay}`}
      style={{ "--stat-accent": accentColor } as React.CSSProperties}
    >
      <span className="fed-stat-icon">{icon}</span>
      <div className="fed-stat-value">{value}</div>
      <div className="fed-stat-label">{label}</div>
      {sub && <div className="fed-stat-sub">{sub}</div>}
      {children}
    </div>
  );
}

// ── Outbox Tab ───────────────────────────────────────────────

function OutboxTab({
  activities,
  loading,
}: {
  activities: APActivity[];
  loading: boolean;
}) {
  if (loading) return <TimelineSkeleton />;

  if (activities.length === 0) {
    return (
      <div className="fed-empty">
        <span className="fed-empty-icon">📤</span>
        <p className="fed-empty-text">Aucune activité publiée</p>
        <p className="fed-empty-sub">
          Les activités apparaîtront ici après la création d&apos;un dépôt ou un
          git push
        </p>
      </div>
    );
  }

  return (
    <div className="fed-timeline">
      {activities.map((activity, i) => {
        const parsed = parseOutboxActivity(activity);
        return (
          <ActivityRow
            key={activity.id || `outbox-${i}`}
            parsed={parsed}
            index={i}
          />
        );
      })}
    </div>
  );
}

// ── Inbox Tab ────────────────────────────────────────────────

function InboxTab({
  activities,
  loading,
  isAuthenticated,
}: {
  activities: InboxActivityItem[];
  loading: boolean;
  isAuthenticated: boolean;
}) {
  // 🔒 Accès refusé — effet verre dépoli
  if (!isAuthenticated) {
    return (
      <div className="fed-inbox-locked">
        <div className="fed-inbox-locked-bg" />
        <div className="fed-inbox-locked-content">
          <span className="fed-inbox-locked-icon">🔒</span>
          <h3 className="fed-inbox-locked-title">Accès refusé</h3>
          <p className="fed-inbox-locked-text">
            Veuillez authentifier votre lien neuronal (JWT) pour consulter ces
            archives.
            <br />
            <Link href="/login">→ Établir la connexion</Link>
          </p>
        </div>
      </div>
    );
  }

  if (loading) return <TimelineSkeleton />;

  if (activities.length === 0) {
    return (
      <div className="fed-empty">
        <span className="fed-empty-icon">📥</span>
        <p className="fed-empty-text">Aucune activité reçue</p>
        <p className="fed-empty-sub">
          Les activités des forges distantes apparaîtront ici
        </p>
      </div>
    );
  }

  return (
    <div className="fed-timeline">
      {activities.map((item, i) => {
        const parsed = parseInboxActivity(item);
        return (
          <InboxActivityRow
            key={item.id}
            parsed={parsed}
            item={item}
            index={i}
          />
        );
      })}
    </div>
  );
}

// ── Followers Tab ────────────────────────────────────────────

function FollowersTab({
  followers,
  loading,
}: {
  followers: string[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="fed-followers-grid">
        {[...Array(3)].map((_, i) => (
          <Skeleton key={i} className="h-12 w-full rounded-lg" />
        ))}
      </div>
    );
  }

  if (followers.length === 0) {
    return (
      <div className="fed-empty">
        <span className="fed-empty-icon">👥</span>
        <p className="fed-empty-text">Aucun follower fédéré</p>
        <p className="fed-empty-sub">
          D&apos;autres instances pourront suivre votre profil via le protocole
          ActivityPub
        </p>
      </div>
    );
  }

  return (
    <div className="fed-followers-grid">
      {followers.map((uri, i) => {
        const segments = uri.replace(/\/$/, "").split("/");
        const handle = segments[segments.length - 1];
        const domain = (() => {
          try {
            return new URL(uri).hostname;
          } catch {
            return "—";
          }
        })();

        return (
          <div
            key={uri}
            className={`fed-follower-item fed-animate-in fed-stagger-${Math.min(i + 1, 4)}`}
          >
            <div className="fed-follower-avatar">
              {handle.charAt(0).toUpperCase()}
            </div>
            <div className="fed-follower-uri" title={uri}>
              <span style={{ fontWeight: 600 }}>@{handle}</span>
              <span style={{ opacity: 0.5, marginLeft: 4 }}>· {domain}</span>
            </div>
            <span className="fed-follower-badge">✓ Accepté</span>
          </div>
        );
      })}
    </div>
  );
}

// ═══════════════════════════════════════════════════════════════
// Shared Atoms
// ═══════════════════════════════════════════════════════════════

/** Outbox activity row — Vegapunk semantic parser output. */
function ActivityRow({
  parsed,
  index,
}: {
  parsed: ParsedActivity;
  index: number;
}) {
  return (
    <div
      className={`fed-timeline-item fed-animate-in fed-stagger-${Math.min(index + 1, 4)}`}
    >
      <div
        className="fed-timeline-icon"
        style={{
          background: `color-mix(in oklch, ${parsed.color} 15%, var(--secondary))`,
        }}
      >
        {parsed.icon}
      </div>
      <div className="fed-timeline-body">
        <div className="fed-timeline-text">
          <span className="fed-actor">{parsed.actorHandle}</span>{" "}
          <span className="fed-verb">{parsed.verb}</span>{" "}
          {parsed.objectName && (
            <span className="fed-object">{parsed.objectName}</span>
          )}
        </div>
        <div className="fed-timeline-meta">
          <span
            className="fed-timeline-type-badge"
            style={
              {
                "--badge-color": parsed.color,
              } as React.CSSProperties
            }
          >
            {parsed.objectType || "Activity"}
          </span>
          <span>
            <RelativeTime iso={parsed.timestamp} />
          </span>
        </div>
      </div>
    </div>
  );
}

/** Inbox activity row — with processed/pending status badge. */
function InboxActivityRow({
  parsed,
  item,
  index,
}: {
  parsed: ParsedActivity;
  item: InboxActivityItem;
  index: number;
}) {
  return (
    <div
      className={`fed-timeline-item fed-animate-in fed-stagger-${Math.min(index + 1, 4)}`}
    >
      <div
        className="fed-timeline-icon"
        style={{
          background: `color-mix(in oklch, ${parsed.color} 15%, var(--secondary))`,
        }}
      >
        {parsed.icon}
      </div>
      <div className="fed-timeline-body">
        <div className="fed-timeline-text">
          <span className="fed-actor">{parsed.actorHandle}</span>{" "}
          <span className="fed-verb">{parsed.verb}</span>{" "}
          {parsed.objectName && (
            <span className="fed-object">{parsed.objectName}</span>
          )}
        </div>
        <div className="fed-timeline-meta">
          <span
            className="fed-timeline-type-badge"
            style={
              {
                "--badge-color": parsed.color,
              } as React.CSSProperties
            }
          >
            {item.activityType}
          </span>
          <span
            className="fed-timeline-status"
            data-status={item.processed ? "processed" : "pending"}
          >
            {item.processed ? "✓ Traité" : "⏳ En attente"}
          </span>
          <span>
            <RelativeTime iso={parsed.timestamp} />
          </span>
        </div>
      </div>
    </div>
  );
}

/** Loading skeleton for timelines. */
function TimelineSkeleton() {
  return (
    <div className="fed-skeleton-timeline">
      {[...Array(5)].map((_, i) => (
        <div key={i} className="fed-skeleton-row">
          <div className="fed-skeleton-circle" />
          <div className="fed-skeleton-lines">
            <div className="fed-skeleton-line" />
            <div className="fed-skeleton-line" />
          </div>
        </div>
      ))}
    </div>
  );
}
