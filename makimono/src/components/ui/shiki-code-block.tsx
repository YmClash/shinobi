import { highlightCode } from "@/lib/shiki";
import { CopyButton } from "@/components/ui/copy-button";

interface ShikiCodeBlockProps {
  code: string;
  language: string;
  startLine?: number;
  filePath?: string;
  className?: string;
}

/**
 * Server Component — renders syntax-highlighted code via Shiki.
 *
 * Uses the `css-variables` theme so the colours automatically adapt
 * to whichever Makimono theme is active (Ninja, Cyberpunk, Glass, Parchemin).
 *
 * Zero kilobytes of highlighting JavaScript are sent to the client.
 * The only client-side JS is the tiny CopyButton.
 */
export async function ShikiCodeBlock({
  code,
  language,
  startLine = 1,
  filePath,
  className = "",
}: ShikiCodeBlockProps) {
  const html = await highlightCode(code, language);

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
          {startLine > 1 && (
            <span className="text-[10px] opacity-60">
              L{startLine}
            </span>
          )}
        </div>
        <CopyButton text={code} />
      </div>

      {/* Highlighted code */}
      <div
        className="shiki-content p-0 overflow-x-auto text-xs leading-5 font-mono"
        style={{ counterReset: `line ${startLine - 1}` }}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}
