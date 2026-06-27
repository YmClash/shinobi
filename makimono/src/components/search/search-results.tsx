"use client";

import { useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { SimilarityBadge } from "@/components/ui/similarity-badge";
import { CodeViewer } from "@/components/search/code-viewer";
import { Skeleton } from "@/components/ui/skeleton";
import type { SemanticChunk } from "@/lib/api";

const KIND_CONFIG: Record<string, { color: string; icon: string }> = {
  function: { color: "bg-blue-500/15 text-blue-400 border-blue-500/30", icon: "ƒ" },
  struct:   { color: "bg-purple-500/15 text-purple-400 border-purple-500/30", icon: "S" },
  enum:     { color: "bg-orange-500/15 text-orange-400 border-orange-500/30", icon: "E" },
  trait:    { color: "bg-cyan-500/15 text-cyan-400 border-cyan-500/30", icon: "T" },
  impl:     { color: "bg-green-500/15 text-green-400 border-green-500/30", icon: "I" },
  import:   { color: "bg-gray-500/15 text-gray-400 border-gray-500/30", icon: "→" },
  module:   { color: "bg-yellow-500/15 text-yellow-400 border-yellow-500/30", icon: "M" },
  comment:  { color: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30", icon: "#" },
  block:    { color: "bg-pink-500/15 text-pink-400 border-pink-500/30", icon: "{}" },
};

interface SearchResultsProps {
  results: SemanticChunk[] | null;
  loading: boolean;
  query: string;
}

export function SearchResults({ results, loading, query }: SearchResultsProps) {
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);

  if (loading) {
    return (
      <div className="space-y-3 mt-4">
        {[...Array(3)].map((_, i) => (
          <Card key={i} className="glass-card">
            <CardContent className="p-4 space-y-2">
              <div className="flex items-center gap-2">
                <Skeleton className="h-5 w-16" />
                <Skeleton className="h-5 w-24" />
                <Skeleton className="h-5 w-12 ml-auto" />
              </div>
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-20 w-full" />
            </CardContent>
          </Card>
        ))}
      </div>
    );
  }

  if (!results || !query) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
        <span className="text-5xl mb-4 opacity-30">🔍</span>
        <p className="text-sm">Interrogez Tensai pour explorer le codebase</p>
        <p className="text-xs mt-1 opacity-60">Exemple : &quot;user creation function&quot;</p>
      </div>
    );
  }

  if (results.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
        <span className="text-5xl mb-4 opacity-30">🤷</span>
        <p className="text-sm">Aucun résultat pour &quot;{query}&quot;</p>
        <p className="text-xs mt-1 opacity-60">Essayez une requête différente ou abaissez le seuil</p>
      </div>
    );
  }

  return (
    <div className="space-y-3 mt-4">
      <p className="text-xs text-muted-foreground">
        {results.length} résultat{results.length > 1 ? "s" : ""} pour &quot;{query}&quot;
      </p>
      {results.map((chunk, i) => {
        const kind = KIND_CONFIG[chunk.kind] ?? { color: "bg-muted text-muted-foreground", icon: "?" };
        const isExpanded = expandedIndex === i;

        return (
          <Card
            key={i}
            className={`glass-card neon-glow cursor-pointer transition-all duration-200 hover:border-primary/30 animate-fade-in-up stagger-${Math.min(i + 1, 5)}`}
            style={{ opacity: 0 }}
            onClick={() => setExpandedIndex(isExpanded ? null : i)}
          >
            <CardContent className="p-4">
              {/* Header row */}
              <div className="flex items-center gap-2 flex-wrap">
                <Badge variant="outline" className={`text-[11px] font-mono px-1.5 py-0 ${kind.color}`}>
                  {kind.icon} {chunk.kind}
                </Badge>
                {chunk.name && (
                  <span className="font-mono font-semibold text-sm">{chunk.name}</span>
                )}
                <SimilarityBadge score={chunk.similarity} className="ml-auto" />
              </div>

              {/* Meta row */}
              <div className="flex items-center gap-3 mt-2 text-xs text-muted-foreground">
                <span className="font-mono">{chunk.file_path}</span>
                <span>L{chunk.start_line}–{chunk.end_line}</span>
              </div>

              {/* Expandable code preview */}
              {isExpanded && (
                <div className="mt-3">
                  <CodeViewer
                    code={chunk.content}
                    language={chunk.language}
                    startLine={chunk.start_line}
                    filePath={chunk.file_path}
                  />
                </div>
              )}
              {!isExpanded && (
                <div className="mt-2 font-mono text-xs text-muted-foreground/70 truncate">
                  {chunk.content.split("\n")[0]}
                </div>
              )}
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
