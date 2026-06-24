"use client";

import { useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { CodeViewer } from "@/components/search/code-viewer";
import { useIpfsContent } from "@/hooks/use-api";
import type { IpfsFile } from "@/lib/api";

// Language icons/colours
const LANG_CONFIG: Record<string, { color: string; icon: string }> = {
  rust:       { color: "bg-orange-500/15 text-orange-400 border-orange-500/30", icon: "🦀" },
  typescript: { color: "bg-blue-500/15 text-blue-400 border-blue-500/30", icon: "TS" },
  tsx:        { color: "bg-blue-500/15 text-blue-400 border-blue-500/30", icon: "⚛" },
  css:        { color: "bg-pink-500/15 text-pink-400 border-pink-500/30", icon: "🎨" },
  python:     { color: "bg-yellow-500/15 text-yellow-400 border-yellow-500/30", icon: "🐍" },
  javascript: { color: "bg-yellow-500/15 text-yellow-400 border-yellow-500/30", icon: "JS" },
  jsx:        { color: "bg-cyan-500/15 text-cyan-400 border-cyan-500/30", icon: "⚛" },
  json:       { color: "bg-gray-500/15 text-gray-400 border-gray-500/30", icon: "{}" },
  toml:       { color: "bg-gray-500/15 text-gray-400 border-gray-500/30", icon: "⚙" },
  yaml:       { color: "bg-purple-500/15 text-purple-400 border-purple-500/30", icon: "📋" },
  markdown:   { color: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30", icon: "📝" },
  html:       { color: "bg-red-500/15 text-red-400 border-red-500/30", icon: "🌐" },
  sql:        { color: "bg-indigo-500/15 text-indigo-400 border-indigo-500/30", icon: "🗄" },
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

interface IpfsExplorerProps {
  operationId: string;
}

export function IpfsExplorer({ operationId }: IpfsExplorerProps) {
  const { data, loading, error } = useIpfsContent(operationId);
  const [selectedFile, setSelectedFile] = useState<IpfsFile | null>(null);

  if (loading) {
    return (
      <div className="space-y-3 mt-4">
        <Skeleton className="h-16 w-full" />
        {[...Array(3)].map((_, i) => (
          <Skeleton key={i} className="h-10 w-full" />
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
        <span className="text-4xl mb-3 opacity-30">⚠️</span>
        <p className="text-sm">{error}</p>
        <p className="text-xs mt-1 opacity-60">L&apos;IPFS n&apos;est peut-être pas disponible</p>
      </div>
    );
  }

  if (!data || data.files.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
        <span className="text-4xl mb-3 opacity-30">📦</span>
        <p className="text-sm">Aucun fichier IPFS pour cette opération</p>
      </div>
    );
  }

  return (
    <div className="space-y-4 mt-4">
      {/* IPFS Overview Card */}
      <Card className="glass-card neon-glow">
        <CardContent className="p-4">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <div className="flex items-center gap-2">
              <span className="text-lg">🌐</span>
              <div>
                <p className="text-xs text-muted-foreground">IPFS CID (Genjutsu)</p>
                <p className="font-mono text-xs">{data.ipfs_cid}</p>
              </div>
            </div>
            <div className="flex gap-3">
              <Badge variant="outline" className="text-[10px] font-mono px-2">
                📦 {formatBytes(data.blob_size)}
              </Badge>
              <Badge variant="outline" className="text-[10px] font-mono px-2">
                📄 {data.count} fichier{data.count > 1 ? "s" : ""}
              </Badge>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* File List */}
      <div className="grid gap-2">
        {data.files.map((file, i) => {
          const lang = file.language ?? "text";
          const config = LANG_CONFIG[lang] ?? { color: "bg-muted text-muted-foreground", icon: "📄" };
          const isSelected = selectedFile?.path === file.path;

          return (
            <div key={i}>
              <Card
                className={`glass-card cursor-pointer transition-all duration-200 hover:border-primary/30 ${
                  isSelected ? "border-primary/50 ring-1 ring-primary/20" : ""
                } animate-fade-in-up stagger-${Math.min(i + 1, 5)}`}
                style={{ opacity: 0 }}
                onClick={() => setSelectedFile(isSelected ? null : file)}
              >
                <CardContent className="p-3">
                  <div className="flex items-center gap-3">
                    <Badge variant="outline" className={`text-[10px] font-mono px-1.5 py-0 ${config.color}`}>
                      {config.icon}
                    </Badge>
                    <span className="font-mono text-xs flex-1 truncate">{file.path}</span>
                    <span className="text-[10px] text-muted-foreground">
                      {formatBytes(file.size)}
                    </span>
                    {file.cid && (
                      <Badge variant="outline" className="text-[9px] font-mono px-1.5 py-0 bg-emerald-500/10 text-emerald-400 border-emerald-500/25 max-w-[120px] truncate" title={file.cid}>
                        🔗 {file.cid.slice(0, 12)}…
                      </Badge>
                    )}
                    {file.language && (
                      <Badge variant="secondary" className="text-[10px] px-1.5 py-0">
                        {file.language}
                      </Badge>
                    )}
                  </div>
                </CardContent>
              </Card>

              {/* Expanded CodeViewer */}
              {isSelected && (
                <div className="mt-2 ml-4 animate-fade-in-up">
                  <CodeViewer
                    code={file.content}
                    language={file.language ?? "text"}
                    filePath={file.path}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
