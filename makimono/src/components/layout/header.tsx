"use client";

import { useHealth } from "@/hooks/use-api";
import { Badge } from "@/components/ui/badge";

export function Header() {
  const { data: health } = useHealth();

  return (
    <header className="h-12 border-b border-border bg-card/50 backdrop-blur-sm flex items-center justify-between px-4 flex-shrink-0">
      <div className="flex items-center gap-3">
        <h1 className="text-sm font-semibold tracking-wide">
          Dashboard de l&apos;Architecte
        </h1>
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
