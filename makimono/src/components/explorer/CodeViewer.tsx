"use client";

// ═══════════════════════════════════════════════════════════════
// CodeViewer — Vue fichier style GitHub × SHINOBI
// Header : nom + langage + taille + boutons Copy/Raw/Tensai
// Corps  : numéros de ligne + Shiki (lazy) ou fallback plain
// ═══════════════════════════════════════════════════════════════

import React, { useEffect, useState } from "react";
import {
  Copy,
  Check,
  Binary,
  FileCode,
  Sparkles,
  Code,
} from "lucide-react";
import { decodeBase64 } from "@/lib/explorer-api";

interface CodeViewerProps {
  path: string;
  contentB64: string;
  language: string | null;
  isText: boolean;
  size: number;
  /** Callback pour ouvrir le panneau Tensai (Phase 14). */
  onTensaiClick?: () => void;
}

export default function CodeViewer({
  path,
  contentB64,
  language,
  isText,
  size,
  onTensaiClick,
}: CodeViewerProps) {
  const [highlightedHtml, setHighlightedHtml] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [rawMode, setRawMode] = useState(false);

  const content = isText ? decodeBase64(contentB64) : null;
  const filename = path.split("/").pop() ?? path;
  const lines = content?.split("\n") ?? [];

  // ── Shiki lazy-load (dual-theme via CSS variables) ────────────
  useEffect(() => {
    if (!content || !isText || rawMode) return;
    let cancelled = false;
    (async () => {
      try {
        const { highlightCode } = await import("@/lib/shiki");
        const html = await highlightCode(content, language ?? "text");
        if (!cancelled) setHighlightedHtml(html);
      } catch {
        if (!cancelled) setHighlightedHtml(null);
      }
    })();
    return () => { cancelled = true; };
  }, [content, language, isText, rawMode]);

  // ── Helpers ──────────────────────────────────────────────────
  const formatSize = (b: number) => {
    if (b < 1024) return `${b} B`;
    if (b < 1048576) return `${(b / 1024).toFixed(1)} KB`;
    return `${(b / 1048576).toFixed(1)} MB`;
  };

  const handleCopy = async () => {
    if (!content) return;
    await navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // ── Binaire ──────────────────────────────────────────────────
  if (!isText) {
    return (
      <div className="cv-root">
        <div className="cv-header">
          <div className="cv-header-left">
            <FileCode size={16} className="cv-filename-icon" />
            <span className="cv-filename">{filename}</span>
            <span className="cv-separator">|</span>
            <span className="cv-meta">{formatSize(size)}</span>
          </div>
        </div>
        <div className="cv-binary">
          <Binary size={52} className="cv-binary-icon" />
          <p className="cv-binary-label">Fichier binaire — {formatSize(size)}</p>
          <p className="cv-binary-hint">Contenu non-texte, affichage impossible</p>
        </div>
      </div>
    );
  }

  return (
    <div className="cv-root">
      {/* ── Header ─────────────────────────────────── */}
      <div className="cv-header">
        <div className="cv-header-left">
          <FileCode size={16} className="cv-filename-icon" />
          <span className="cv-filename">{filename}</span>
          <span className="cv-separator">|</span>
          {language && <span className="cv-lang-badge">{language}</span>}
          <span className="cv-meta">
            {lines.length} lignes · {formatSize(size)}
          </span>
        </div>
        <div className="cv-actions">
          {/* Copier */}
          <button
            className="cv-btn-icon"
            onClick={handleCopy}
            title="Copier le contenu"
          >
            {copied ? <Check size={15} /> : <Copy size={15} />}
          </button>

          {/* Raw / Highlighted toggle */}
          <button
            className={`cv-btn cv-btn-raw ${rawMode ? "cv-btn-active" : ""}`}
            onClick={() => setRawMode((r) => !r)}
            title="Affichage brut"
          >
            <Code size={14} />
            <span>Raw</span>
          </button>

          {/* Tensai IA */}
          <button
            className="cv-btn cv-btn-tensai"
            title="Analyser avec Tensai"
            onClick={onTensaiClick}
          >
            <Sparkles size={14} />
            <span>Demander à Sensai</span>
          </button>
        </div>
      </div>

      {/* ── Corps du code ──────────────────────────── */}
      <div className="cv-body">
        {!rawMode && highlightedHtml ? (
          // Shiki rendu
          <div
            className="cv-shiki"
            dangerouslySetInnerHTML={{ __html: highlightedHtml }}
          />
        ) : (
          // Fallback plain (ou raw forcé)
          <div className="cv-plain">
            {/* Numéros de ligne */}
            <div className="cv-gutter" aria-hidden="true">
              {lines.map((_, i) => (
                <span key={i} className="cv-ln">
                  {i + 1}
                </span>
              ))}
            </div>
            {/* Code */}
            <pre className="cv-pre">
              <code>{content}</code>
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}
