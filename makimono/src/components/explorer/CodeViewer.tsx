"use client";

import React, { useEffect, useState } from "react";
import { Copy, Check, Binary } from "lucide-react";
import { decodeBase64 } from "@/lib/explorer-api";

interface CodeViewerProps {
  path: string;
  contentB64: string;
  language: string | null;
  isText: boolean;
  size: number;
}

export default function CodeViewer({
  path,
  contentB64,
  language,
  isText,
  size,
}: CodeViewerProps) {
  const [highlightedHtml, setHighlightedHtml] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const content = isText ? decodeBase64(contentB64) : null;
  const filename = path.split("/").pop() ?? path;

  useEffect(() => {
    if (!content || !isText) return;

    let cancelled = false;

    const highlight = async () => {
      try {
        const { codeToHtml } = await import("shiki");
        const html = await codeToHtml(content, {
          lang: language ?? "text",
          theme: "github-dark",
        });
        if (!cancelled) setHighlightedHtml(html);
      } catch {
        // fallback : pas de coloration
        if (!cancelled) setHighlightedHtml(null);
      }
    };

    highlight();
    return () => {
      cancelled = true;
    };
  }, [content, language, isText]);

  const handleCopy = async () => {
    if (!content) return;
    await navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  // Fichier binaire
  if (!isText) {
    return (
      <div className="code-viewer">
        <div className="code-header">
          <span className="code-filename">{filename}</span>
          <span className="code-meta">{formatSize(size)}</span>
        </div>
        <div className="code-binary">
          <Binary size={48} className="binary-icon" />
          <p>Fichier binaire — {formatSize(size)}</p>
          <p className="binary-hint">Impossible d&apos;afficher le contenu binaire</p>
        </div>
      </div>
    );
  }

  const lines = content?.split("\n") ?? [];

  return (
    <div className="code-viewer">
      {/* Header */}
      <div className="code-header">
        <div className="code-header-left">
          <span className="code-filename">{filename}</span>
          {language && (
            <span className="code-lang-badge">{language}</span>
          )}
          <span className="code-meta">
            {lines.length} lignes · {formatSize(size)}
          </span>
        </div>
        <button
          className="code-copy-btn"
          onClick={handleCopy}
          title="Copier le contenu"
        >
          {copied ? (
            <>
              <Check size={14} />
              <span>Copié !</span>
            </>
          ) : (
            <>
              <Copy size={14} />
              <span>Copier</span>
            </>
          )}
        </button>
      </div>

      {/* Code */}
      <div className="code-body">
        {highlightedHtml ? (
          <div
            className="shiki-wrapper"
            dangerouslySetInnerHTML={{ __html: highlightedHtml }}
          />
        ) : (
          <div className="code-plain">
            <div className="line-numbers" aria-hidden="true">
              {lines.map((_, i) => (
                <span key={i} className="line-num">
                  {i + 1}
                </span>
              ))}
            </div>
            <pre className="code-pre">
              <code>{content}</code>
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}
