"use client";

// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo] Layout — GitHub-style repo tab bar (Phase 26B)
// Wraps all repo sub-pages with a horizontal navigation bar
// ═══════════════════════════════════════════════════════════════

import { useParams } from "next/navigation";
import { RepoTabBar } from "@/components/layout/repo-tab-bar";

export default function RepoLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const params = useParams<{ owner: string; repo: string }>();

  return (
    <div className="flex flex-col h-full">
      <RepoTabBar owner={params.owner} repo={params.repo} />
      <div className="flex-1 overflow-y-auto">
        {children}
      </div>
    </div>
  );
}
