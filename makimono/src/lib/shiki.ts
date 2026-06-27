// ═══════════════════════════════════════════════════════════════
// SHINOBI — Makimono · Shiki Singleton Highlighter
// Server-side only — uses dual themes (vitesse-dark/light) for Design System
// ═══════════════════════════════════════════════════════════════

import {
  createHighlighter,
  type Highlighter,
  type BundledLanguage,
} from "shiki";

// ── Singleton pattern ────────────────────────────────────────
// Prevents re-initialising the highlighter on every server request.
// Next.js module-scope caching keeps one instance across hot reloads.

let highlighterPromise: Promise<Highlighter> | null = null;

/**
 * Languages pre-loaded by the highlighter.
 * Shiki lazy-loads grammars — only these are bundled up-front.
 */
const PRELOADED_LANGS: BundledLanguage[] = [
  "rust",
  "typescript",
  "tsx",
  "css",
  "python",
  "json",
  "toml",
  "yaml",
  "markdown",
  "bash",
  "sql",
  "html",
  "javascript",
  "jsx",
  "diff",
];

function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: ["vitesse-dark", "vitesse-light"],
      langs: PRELOADED_LANGS,
    });
  }
  return highlighterPromise;
}

/**
 * Maps file extensions / language identifiers to Shiki BundledLanguage.
 * Falls back to "text" for unknown languages.
 */
function normalizeLanguage(lang: string): BundledLanguage {
  const map: Record<string, BundledLanguage> = {
    rust: "rust",
    rs: "rust",
    typescript: "typescript",
    ts: "typescript",
    tsx: "tsx",
    javascript: "javascript",
    js: "javascript",
    jsx: "jsx",
    css: "css",
    python: "python",
    py: "python",
    json: "json",
    toml: "toml",
    yaml: "yaml",
    yml: "yaml",
    markdown: "markdown",
    md: "markdown",
    bash: "bash",
    sh: "bash",
    sql: "sql",
    html: "html",
    diff: "diff",
  };
  return map[lang.toLowerCase()] ?? ("text" as BundledLanguage);
}

/**
 * Highlights source code server-side via Shiki.
 *
 * Uses dual themes (vitesse-dark / vitesse-light) with CSS
 * variable overrides so colours adapt to all 4 Makimono themes.
 *
 * @returns HTML string with `<pre class="shiki">` wrapper.
 */
export async function highlightCode(
  code: string,
  language: string,
): Promise<string> {
  const hl = await getHighlighter();
  const lang = normalizeLanguage(language);

  return hl.codeToHtml(code, {
    lang,
    themes: {
      dark: "vitesse-dark",
      light: "vitesse-light",
    },
  });
}
