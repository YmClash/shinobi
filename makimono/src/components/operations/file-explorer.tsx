"use client";

import { useState, useMemo } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { CodeViewer } from "@/components/search/code-viewer";
import type { Chunk } from "@/lib/api";

interface FileExplorerProps {
  chunks: Chunk[];
  className?: string;
}

/**
 * 2-column file explorer: file list on left, chunk viewer on right.
 * Groups chunks by file_path and allows clicking files to inspect their semantic fragments.
 */
export function FileExplorer({ chunks, className = "" }: FileExplorerProps) {
  // Group chunks by file_path
  const fileGroups = useMemo(() => {
    const groups = new Map<string, Chunk[]>();
    for (const chunk of chunks) {
      const existing = groups.get(chunk.file_path) ?? [];
      existing.push(chunk);
      groups.set(chunk.file_path, existing);
    }
    return groups;
  }, [chunks]);

  const files = useMemo(() => Array.from(fileGroups.keys()).sort(), [fileGroups]);
  const [selectedFile, setSelectedFile] = useState<string | null>(files[0] ?? null);

  const selectedChunks = selectedFile ? fileGroups.get(selectedFile) ?? [] : [];

  // Count chunk types across all
  const chunkTypeCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const chunk of chunks) {
      counts.set(chunk.kind, (counts.get(chunk.kind) ?? 0) + 1);
    }
    return counts;
  }, [chunks]);

  if (chunks.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
        <span className="text-4xl mb-3 opacity-30">🧩</span>
        <p className="text-sm">Aucun chunk sémantique trouvé</p>
        <p className="text-xs mt-1 opacity-60">
          Tensai n&apos;a pas encore analysé cette opération
        </p>
      </div>
    );
  }

  return (
    <div className={`space-y-4 ${className}`}>
      {/* Summary bar */}
      <div className="flex flex-wrap gap-1.5 items-center text-xs text-muted-foreground">
        <span className="font-medium text-foreground">
          {chunks.length} chunk{chunks.length > 1 ? "s" : ""}
        </span>
        <span>·</span>
        <span>{files.length} fichier{files.length > 1 ? "s" : ""}</span>
        <span>·</span>
        {Array.from(chunkTypeCounts.entries()).map(([kind, count]) => (
          <Badge key={kind} variant="outline" className="text-[10px] px-1.5 py-0">
            {kind}: {count}
          </Badge>
        ))}
      </div>

      {/* 2-column layout */}
      <div className="grid grid-cols-1 md:grid-cols-[220px_1fr] gap-3">
        {/* File list sidebar */}
        <Card className="glass-card h-fit">
          <CardHeader className="pb-2 pt-3 px-3">
            <CardTitle className="text-xs font-medium text-muted-foreground">
              📁 Fichiers
            </CardTitle>
          </CardHeader>
          <CardContent className="px-2 pb-2 space-y-0.5">
            {files.map((file) => {
              const fileChunks = fileGroups.get(file) ?? [];
              const isActive = file === selectedFile;
              return (
                <button
                  key={file}
                  onClick={() => setSelectedFile(file)}
                  className={`
                    w-full text-left px-2 py-1.5 rounded text-xs font-mono truncate
                    transition-colors cursor-pointer
                    ${isActive
                      ? "bg-primary/15 text-primary border border-primary/20"
                      : "hover:bg-accent/50 text-muted-foreground"
                    }
                  `}
                  title={file}
                >
                  <span className="truncate block">{file}</span>
                  <span className="text-[10px] opacity-60">
                    {fileChunks.length} chunk{fileChunks.length > 1 ? "s" : ""}
                  </span>
                </button>
              );
            })}
          </CardContent>
        </Card>

        {/* Chunk viewer */}
        <div className="space-y-3">
          {selectedFile && (
            <div className="text-xs font-mono text-muted-foreground px-1">
              📄 {selectedFile}
            </div>
          )}
          {selectedChunks.map((chunk, i) => (
            <Card key={i} className="glass-card overflow-hidden">
              <CardHeader className="pb-1 pt-2 px-3">
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="text-[10px] px-1.5 py-0">
                    {chunk.kind}
                  </Badge>
                  {chunk.name && (
                    <span className="text-xs font-mono font-medium truncate">
                      {chunk.name}
                    </span>
                  )}
                  <span className="text-[10px] text-muted-foreground ml-auto">
                    L{chunk.start_line}–{chunk.end_line}
                  </span>
                </div>
              </CardHeader>
              <CardContent className="p-0">
                <CodeViewer
                  code={chunk.content}
                  language={chunk.language}
                  startLine={chunk.start_line}
                />
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    </div>
  );
}
