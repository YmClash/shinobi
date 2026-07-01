"use client";

import { usePathname } from "next/navigation";
import { useHealth } from "@/hooks/use-api";
import { Badge } from "@/components/ui/badge";

export function Header() {
  const { data: health } = useHealth();
  const pathname = usePathname();

  // Detect repo context from URL: /[owner]/[repo]/...
  const segments = pathname.split("/").filter(Boolean);
  const isRepoContext =
    segments.length >= 3 && segments[2] === "operations";
  const repoOwner = isRepoContext ? segments[0] : null;
  const repoName = isRepoContext ? segments[1] : null;

  return (
    <header className="h-12 border-b border-border bg-card/50 backdrop-blur-sm flex items-center justify-between px-4 flex-shrink-0">
      <div className="flex items-center gap-3">
        <h1 className="text-sm font-semibold tracking-wide">
          Dashboard de l&apos;Architecte
        </h1>
        {repoOwner && repoName && (
          <span className="repo-breadcrumb">
            <span>📦</span>
            <span>{repoOwner}/{repoName}</span>
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {health?.version && (
          <Badge variant="outline" className="text-[10px] font-mono px-2 py-0.5">
            Taijutsu {health.version}
          </Badge>
        )}
        <Badge variant="secondary" className="text-[10px] font-mono px-2 py-0.5">
          Makimono v0.1.0
        </Badge>
      </div>
    </header>
  );
}
