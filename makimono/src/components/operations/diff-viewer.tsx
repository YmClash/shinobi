"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface DiffViewerProps {
  changedFiles: string[];
  className?: string;
}

/**
 * Renders a unified diff view showing changed files.
 * Formats as a .diff-style text block with file additions.
 */
export function DiffViewer({ changedFiles, className = "" }: DiffViewerProps) {
  if (changedFiles.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
        <span className="text-4xl mb-3 opacity-30">📄</span>
        <p className="text-sm">Aucun fichier modifié détecté</p>
        <p className="text-xs mt-1 opacity-60">
          Ce commit est peut-être un commit de métadonnées (empty tree)
        </p>
      </div>
    );
  }

  // Build a unified diff-style text
  const diffLines: { text: string; type: "header" | "add" | "meta" | "separator" }[] = [];

  diffLines.push({
    text: `diff --shinobi ${changedFiles.length} file(s) changed`,
    type: "header",
  });
  diffLines.push({ text: "---", type: "separator" });

  for (const file of changedFiles) {
    diffLines.push({ text: `--- /dev/null`, type: "meta" });
    diffLines.push({ text: `+++ b/${file}`, type: "add" });
    diffLines.push({ text: `@@ -0,0 +1 @@`, type: "meta" });
    diffLines.push({ text: `+ ${file}`, type: "add" });
    diffLines.push({ text: "", type: "separator" });
  }

  return (
    <Card className={`glass-card ${className}`}>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium flex items-center gap-2">
          <span>📝</span>
          Diff Unifié
          <span className="text-xs font-normal text-muted-foreground ml-auto">
            {changedFiles.length} fichier{changedFiles.length > 1 ? "s" : ""} modifié{changedFiles.length > 1 ? "s" : ""}
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="rounded-lg bg-background/50 border border-border/50 overflow-hidden">
          {/* File list summary */}
          <div className="px-3 py-2 border-b border-border/50 bg-muted/30">
            <div className="flex flex-wrap gap-1.5">
              {changedFiles.map((file) => (
                <span
                  key={file}
                  className="inline-flex items-center gap-1 font-mono text-[11px] bg-green-500/10 text-green-400 px-1.5 py-0.5 rounded"
                >
                  <span className="text-green-500">+</span>
                  {file}
                </span>
              ))}
            </div>
          </div>

          {/* Diff content */}
          <pre className="p-3 overflow-x-auto text-xs font-mono leading-relaxed">
            {diffLines.map((line, i) => (
              <div
                key={i}
                className={`
                  px-2 -mx-2
                  ${line.type === "header" ? "text-chart-1 font-semibold" : ""}
                  ${line.type === "add" ? "bg-green-500/5 text-green-400" : ""}
                  ${line.type === "meta" ? "text-muted-foreground/60 italic" : ""}
                  ${line.type === "separator" ? "h-1" : ""}
                `}
              >
                {line.text}
              </div>
            ))}
          </pre>
        </div>
      </CardContent>
    </Card>
  );
}
