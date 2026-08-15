"use client";

// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo] Layout — GitHub-style repo tab bar (Phase 26B)
// Updated Phase 37B: Fork badge integration
// ═══════════════════════════════════════════════════════════════

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import Link from "next/link";
import { RepoTabBar } from "@/components/layout/repo-tab-bar";
import { getBaseUrl, type Repository } from "@/lib/api";
import { getToken } from "@/lib/auth";

export default function RepoLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const params = useParams<{ owner: string; repo: string }>();
  const [repo, setRepo] = useState<Repository | null>(null);

  const token = getToken();

  // Fetch repo metadata (for fork badge)
  useEffect(() => {
    if (!params.owner || !params.repo) return;

    const url = `${getBaseUrl()}/api/v1/repos/${encodeURIComponent(params.owner)}/${encodeURIComponent(params.repo)}`;
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (token) headers["Authorization"] = `Bearer ${token}`;

    fetch(url, { headers })
      .then((res) => (res.ok ? res.json() : null))
      .then((data: Repository | null) => {
        if (data) setRepo(data);
      })
      .catch(() => {});
  }, [params.owner, params.repo, token]);

  const isFork = Boolean(repo?.forked_from_id);
  const parentOwner = repo?.forked_from_owner;
  const parentName = repo?.forked_from_name;
  const hasParentInfo = parentOwner && parentName;

  return (
    <div className="flex flex-col h-full">
      {/* Fork badge — GitHub-style "Forked from owner/repo" (above tabs) */}
      {isFork && (
        <div className="px-4 py-1 text-[11px] text-muted-foreground/60 flex items-center gap-1.5">
          <span>🍴</span>
          <span>Forked from</span>
          {hasParentInfo ? (
            <Link
              href={`/${parentOwner}/${parentName}`}
              className="text-primary/70 hover:text-primary hover:underline transition-colors font-medium"
            >
              {parentOwner}/{parentName}
            </Link>
          ) : (
            <span className="font-mono text-muted-foreground/40">
              {repo?.forked_from_id?.slice(0, 8)}…
            </span>
          )}
        </div>
      )}

      {/* Tab bar (fork button removed — now in the repo header actions) */}
      <RepoTabBar owner={params.owner} repo={params.repo} />

      <div className="flex-1 overflow-y-auto">
        {children}
      </div>
    </div>
  );
}
