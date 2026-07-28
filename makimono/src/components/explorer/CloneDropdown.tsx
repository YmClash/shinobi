"use client";

// ═══════════════════════════════════════════════════════════════
// CloneDropdown — Popup avec les URLs de clonage (Phase 26)
// Affiche Git HTTP URL + commandes jj git clone / git clone
// ═══════════════════════════════════════════════════════════════

import { useState, useRef, useEffect } from "react";
import { Download, Copy, Check } from "lucide-react";

interface CloneDropdownProps {
  owner: string;
  repo: string;
}

export default function CloneDropdown({ owner, repo }: CloneDropdownProps) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  // URL Git HTTP
  const httpUrl = `http://localhost:3000/${owner}/${repo}.git`;

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleCopy = async (text: string, key: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(key);
    setTimeout(() => setCopied(null), 2000);
  };

  return (
    <div className="clone-dropdown-wrapper" ref={ref}>
      <button
        className="ex-btn ex-btn-emerald"
        onClick={() => setOpen(!open)}
      >
        <Download size={14} />
        <span>Cloner</span>
      </button>

      {open && (
        <div className="clone-dropdown animate-fade-in-up">
          <div className="clone-dropdown-header">
            <span className="clone-dropdown-title">Cloner ce dépôt</span>
          </div>

          {/* ── HTTP URL ──────────────────────────── */}
          <div className="clone-section">
            <label className="clone-label">HTTPS</label>
            <div className="clone-url-row">
              <input
                type="text"
                readOnly
                value={httpUrl}
                className="clone-url-input"
                onFocus={(e) => e.target.select()}
              />
              <button
                className="clone-copy-btn"
                onClick={() => handleCopy(httpUrl, "url")}
                title="Copier l'URL"
              >
                {copied === "url" ? <Check size={14} /> : <Copy size={14} />}
              </button>
            </div>
          </div>

          {/* ── Commandes ─────────────────────────── */}
          <div className="clone-section">
            <label className="clone-label">Jujutsu (recommandé)</label>
            <div className="clone-cmd-row">
              <code className="clone-cmd">jj git clone {httpUrl}</code>
              <button
                className="clone-copy-btn"
                onClick={() => handleCopy(`jj git clone ${httpUrl}`, "jj")}
                title="Copier la commande"
              >
                {copied === "jj" ? <Check size={14} /> : <Copy size={14} />}
              </button>
            </div>
          </div>

          <div className="clone-section">
            <label className="clone-label">Git</label>
            <div className="clone-cmd-row">
              <code className="clone-cmd">git clone {httpUrl}</code>
              <button
                className="clone-copy-btn"
                onClick={() => handleCopy(`git clone ${httpUrl}`, "git")}
                title="Copier la commande"
              >
                {copied === "git" ? <Check size={14} /> : <Copy size={14} />}
              </button>
            </div>
          </div>

          <div className="clone-footer">
            🔑 Utilisez votre <a href="/settings/tokens" className="clone-footer-link">PAT</a> comme mot de passe
          </div>
        </div>
      )}
    </div>
  );
}
