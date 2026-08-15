"use client";

import type { ProfileActivity } from "@/lib/api";

// ═══════════════════════════════════════════════════════════════
// Phase 37A — Profile Timeline (Outbox ActivityPub)
//
// Client Component — affiche les activités récentes d'un acteur.
// Filtrées côté backend : Push, Create, Update, Accept uniquement.
// ═══════════════════════════════════════════════════════════════

function activityIcon(type: string, objectType: string): string {
  if (type === "Push") return "🚀";
  if (type === "Create" && objectType === "Repository") return "📦";
  if (type === "Create") return "✨";
  if (type === "Update") return "📝";
  if (type === "Accept") return "🤝";
  return "📋";
}

function activityLabel(type: string, objectType: string): string {
  if (type === "Push") return "Push de commits";
  if (type === "Create" && objectType === "Repository") return "Nouveau dépôt créé";
  if (type === "Create" && objectType === "Follow") return "Nouveau follower";
  if (type === "Create") return `Création de ${objectType}`;
  if (type === "Update") return `Mise à jour de ${objectType}`;
  if (type === "Accept" && objectType === "Follow") return "Follow accepté";
  if (type === "Accept") return `${objectType} accepté`;
  return `${type} — ${objectType}`;
}

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "à l'instant";
  if (minutes < 60) return `il y a ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `il y a ${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `il y a ${days}j`;
  const months = Math.floor(days / 30);
  return `il y a ${months} mois`;
}

interface ProfileTimelineProps {
  activities: ProfileActivity[];
}

export function ProfileTimeline({ activities }: ProfileTimelineProps) {
  if (activities.length === 0) return null;

  return (
    <div className="space-y-1">
      {activities.map((activity, i) => (
        <div
          key={`${activity.type}-${activity.published}-${i}`}
          className={`
            group flex items-center gap-3 px-4 py-2.5
            rounded-lg border border-transparent
            transition-all duration-200
            hover:border-border/50 hover:bg-card
            animate-fade-in-up stagger-${Math.min(i + 1, 5)}
          `}
        >
          {/* Timeline dot + connector */}
          <div className="relative flex flex-col items-center">
            <div className="w-8 h-8 rounded-lg bg-primary/10 flex items-center justify-center text-sm group-hover:bg-primary/20 transition-colors">
              {activityIcon(activity.type, activity.object_type)}
            </div>
            {i < activities.length - 1 && (
              <div className="w-px h-3 bg-border/30 mt-1" />
            )}
          </div>

          {/* Content */}
          <div className="flex-1 min-w-0">
            <p className="text-xs font-medium text-foreground">
              {activityLabel(activity.type, activity.object_type)}
            </p>
            {activity.object_id && (
              <p className="text-[10px] text-muted-foreground/60 font-mono truncate mt-0.5">
                {activity.object_id.replace(/^https?:\/\/[^/]+/, "")}
              </p>
            )}
          </div>

          {/* Timestamp */}
          <span className="text-[10px] text-muted-foreground/50 shrink-0">
            {timeAgo(activity.published)}
          </span>
        </div>
      ))}
    </div>
  );
}
