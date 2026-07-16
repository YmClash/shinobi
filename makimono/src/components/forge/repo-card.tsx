"use client";

import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import type { Repository } from "@/lib/api";

// ═══════════════════════════════════════════════════════════════
// RepoCard — Visual card for a repository in the Forge listing
// ═══════════════════════════════════════════════════════════════

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

interface RepoCardProps {
  repo: Repository;
  ownerHandle: string;
  className?: string;
}

export function RepoCard({ repo, ownerHandle, className = "" }: RepoCardProps) {
  const isPublic = repo.visibility === "public";

  return (
    <Link href={`/${ownerHandle}/${repo.name}`}>
      <div
        className={`
          forge-card group relative
          rounded-xl border border-border/50 bg-card p-4
          transition-all duration-300
          hover:border-primary/40 hover:shadow-lg hover:shadow-primary/5
          hover:translate-y-[-2px]
          cursor-pointer
          ${className}
        `}
      >
        {/* Accent bar */}
        <div className="forge-card-accent" />

        {/* Header: name + visibility */}
        <div className="flex items-start justify-between gap-2 mb-2">
          <div className="min-w-0 flex-1">
            <h3 className="text-sm font-semibold text-foreground truncate group-hover:text-primary transition-colors">
              {repo.display_name}
            </h3>
            <p className="text-[10px] font-mono text-muted-foreground/60 mt-0.5">
              {ownerHandle}/{repo.name}
            </p>
          </div>

          <Badge
            variant="outline"
            className={`text-[10px] px-1.5 py-0 shrink-0 ${
              isPublic
                ? "border-emerald-500/30 text-emerald-500/80 bg-emerald-500/5"
                : "border-amber-500/30 text-amber-500/80 bg-amber-500/5"
            }`}
          >
            {isPublic ? "🌍 Public" : "🔒 Private"}
          </Badge>
        </div>

        {/* Description */}
        <p className="text-xs text-muted-foreground line-clamp-2 mb-3 min-h-[2rem]">
          {repo.description || "Aucune description"}
        </p>

        {/* Footer: branch + date */}
        <div className="flex items-center justify-between text-[10px] text-muted-foreground/60">
          <div className="flex items-center gap-1.5">
            <span className="inline-block w-2 h-2 rounded-full bg-primary/50" />
            <span className="font-mono">{repo.default_branch}</span>
          </div>
          <span>{timeAgo(repo.created_at)}</span>
        </div>
      </div>
    </Link>
  );
}
