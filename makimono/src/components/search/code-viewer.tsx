"use client";

import { Card, CardContent } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import { useState } from "react";

interface CodeViewerProps {
  code: string;
  language: string;
  startLine?: number;
  endLine?: number;
  filePath?: string;
  className?: string;
}

export function CodeViewer({
  code,
  language,
  startLine = 1,
  filePath,
  className = "",
}: CodeViewerProps) {
  const [copied, setCopied] = useState(false);
  const lines = code.split("\n");

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback — ignore
    }
  };

  return (
    <Card className={`overflow-hidden glass-card ${className}`}>
      {/* Header bar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-muted/50 border-b border-border">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {filePath && (
            <span className="font-mono truncate max-w-[200px]">{filePath}</span>
          )}
          <span className="px-1.5 py-0.5 rounded bg-primary/10 text-primary text-[10px] font-mono">
            {language}
          </span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleCopy}
          className="h-6 px-2 text-[10px] text-muted-foreground hover:text-foreground"
        >
          {copied ? "✓ Copié" : "Copier"}
        </Button>
      </div>

      {/* Code content */}
      <ScrollArea className="max-h-72">
        <div className="p-3 font-mono text-xs leading-5 overflow-x-auto">
          <table className="border-collapse w-full">
            <tbody>
              {lines.map((line, i) => (
                <tr key={i} className="hover:bg-muted/30 transition-colors">
                  <td className="pr-4 text-right text-muted-foreground/50 select-none w-8 align-top">
                    {startLine + i}
                  </td>
                  <td className="whitespace-pre">{line || " "}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </ScrollArea>
    </Card>
  );
}
