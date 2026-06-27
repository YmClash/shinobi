"use client";

import { useEffect, useState } from "react";
import { CopyButton } from "@/components/ui/copy-button";
import type { Highlighter } from "shiki";

interface CodeViewerProps {
  code: string;
  language: string;
  startLine?: number;
  endLine?: number;
  filePath?: string;
  className?: string;
}

// ── Client-side singleton ────────────────────────────

let highlighterPromise: Promise<Highlighter> | null = null;
const loadedLangs = new Set<string>();

async function getHighlighter(lang: string): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = import("shiki").then(({ createHighlighter }) =>
      createHighlighter({
        themes: ["vitesse-dark", "vitesse-light"],
        langs: [lang],
      })
    );
    const hl = await highlighterPromise;
    loadedLangs.add(lang);
    return hl;
  }

  const hl = await highlighterPromise;

  if (!loadedLangs.has(lang)) {
    try {
      await hl.loadLanguage(lang as Parameters<typeof hl.loadLanguage>[0]);
    } catch {
      // Fallback to plaintext if grammar not found
    }
    loadedLangs.add(lang);
  }

  return hl;
}

/**
 * Client-side code viewer with Shiki-powered syntax highlighting.
 *
 * Fetches highlighted HTML dynamically from a lightweight helper
 * that calls the Shiki highlighter. The css-variables theme lets
 * colours adapt to the active Makimono theme automatically.
 *
 * This component stays "use client" because it lives inside
 * stateful parents (SearchResults, FileExplorer) that manage
 * expansion and file selection.
 */
export function CodeViewer({
  code,
  language,
  startLine = 1,
  filePath,
  className = "",
}: CodeViewerProps) {
  const [highlightedHtml, setHighlightedHtml] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function highlight() {
      try {
        const lang = normalizeLanguage(language);
        const hl = await getHighlighter(lang);
        const html = hl.codeToHtml(code, {
          lang,
          themes: {
            dark: "vitesse-dark",
            light: "vitesse-light",
          },
        });
        if (!cancelled) setHighlightedHtml(html);
      } catch (err) {
        console.warn("[CodeViewer] Shiki failed, using fallback:", err);
        if (!cancelled) setHighlightedHtml(null);
      }
    }

    highlight();
    return () => { cancelled = true; };
  }, [code, language]);

  const lines = code.split("\n");

  return (
    <div className={`shiki-block overflow-hidden rounded-lg border border-border/50 ${className}`}>
      {/* Header bar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-muted/50 border-b border-border/50">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {filePath && (
            <span className="font-mono truncate max-w-[200px]">{filePath}</span>
          )}
          <span className="px-1.5 py-0.5 rounded bg-primary/10 text-primary text-[10px] font-mono">
            {language}
          </span>
        </div>
        <CopyButton text={code} />
      </div>

      {/* Code content */}
      <div className="shiki-content max-h-72 overflow-auto">
        {highlightedHtml ? (
          <div
            className="p-0 overflow-x-auto text-xs leading-5 font-mono"
            style={{ counterReset: `line ${startLine - 1}` }}
            dangerouslySetInnerHTML={{ __html: highlightedHtml }}
          />
        ) : (
          /* Fallback: plain text with line numbers */
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
        )}
      </div>
    </div>
  );
}

// ── Language normaliser (shared with shiki.ts) ──────────────

type ShikiLang = string;

function normalizeLanguage(lang: string): ShikiLang {
  const map: Record<string, string> = {
    rust: "rust", rs: "rust",
    typescript: "typescript", ts: "typescript",
    tsx: "tsx",
    javascript: "javascript", js: "javascript",
    jsx: "jsx",
    css: "css",
    python: "python", py: "python",
    json: "json", toml: "toml",
    yaml: "yaml", yml: "yaml",
    markdown: "markdown", md: "markdown",
    bash: "bash", sh: "bash",
    sql: "sql", html: "html",
    diff: "diff",
  };
  return map[lang.toLowerCase()] ?? "text";
}
