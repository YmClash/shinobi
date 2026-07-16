"use client";

// ═══════════════════════════════════════════════════════════════
// FileBrowser — Vue répertoire style GitHub × SHINOBI
// Affiche la liste des fichiers avec commit info par ligne,
// en-tête dernière révision avec score Oracle.
// ═══════════════════════════════════════════════════════════════

import React from "react";
import { useRouter } from "next/navigation";
import {
  Folder,
  File,
  FileCode,
  FileText,
  GitCommit,
  BrainCircuit,
  GitBranch,
  History,
} from "lucide-react";
import type { TreeEntryItem } from "@/lib/explorer-api";
import { buildTreeUrl } from "@/lib/explorer-api";

interface LatestCommit {
  hash: string;
  message: string;
  author: string;
  date: string;
  oracleScore: number | null;
}

interface FileBrowserProps {
  owner: string;
  repo: string;
  revision: string;
  entries: TreeEntryItem[];
  latestCommit?: LatestCommit;
  branchCount?: number;
  commitCount?: number;
}

/** Avatar initiale colorée */
function Avatar({ name }: { name: string }) {
  return (
    <div className="fb-avatar" aria-hidden="true">
      {name.charAt(0).toUpperCase()}
    </div>
  );
}

/** Icône selon le type et l'extension */
function EntryIcon({ entry }: { entry: TreeEntryItem }) {
  if (entry.kind === "directory") {
    return <Folder size={16} className="fb-icon fb-icon-dir" />;
  }
  const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
  if (["ts", "tsx", "js", "jsx", "rs", "py", "go", "c", "cpp"].includes(ext)) {
    return <FileCode size={16} className="fb-icon fb-icon-code" />;
  }
  if (["md", "txt", "json", "toml", "yaml", "yml", "env"].includes(ext)) {
    return <FileText size={16} className="fb-icon fb-icon-text" />;
  }
  return <File size={16} className="fb-icon fb-icon-file" />;
}

/** Formate la taille */
function formatSize(bytes: number | null): string {
  if (bytes === null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

export default function FileBrowser({
  owner,
  repo,
  revision,
  entries,
  latestCommit,
  branchCount = 0,
  commitCount = 0,
}: FileBrowserProps) {
  const router = useRouter();

  const handleNavigate = (entry: TreeEntryItem) => {
    router.push(buildTreeUrl(owner, repo, revision, entry.path));
  };

  return (
    <div className="fb-root">
      {/* ── Controls Bar ─────────────────────────── */}
      <div className="fb-controls">
        <div className="fb-controls-left">
          <div className="fb-stat">
            <GitBranch size={14} className="fb-stat-icon" />
            <span>
              {branchCount} Bookmark{branchCount !== 1 ? "s" : ""}
            </span>
          </div>
          <div className="fb-stat">
            <History size={14} className="fb-stat-icon" />
            <span>
              {commitCount} Commit{commitCount !== 1 ? "s" : ""}
            </span>
          </div>
        </div>
      </div>

      {/* ── Listing Panel ────────────────────────── */}
      <div className="fb-panel">
        {/* Dernière révision */}
        {latestCommit && (
          <div className="fb-commit-bar">
            <div className="fb-commit-left">
              <Avatar name={latestCommit.author} />
              <span className="fb-commit-author">{latestCommit.author}</span>
              <span className="fb-commit-msg">{latestCommit.message}</span>
            </div>
            <div className="fb-commit-right">
              {latestCommit.oracleScore !== null && (
                <div className="fb-oracle-badge">
                  <BrainCircuit size={14} />
                  <span>Score: {latestCommit.oracleScore}/100</span>
                </div>
              )}
              <div className="fb-commit-hash">
                <GitCommit size={14} />
                <span>{latestCommit.hash.slice(0, 8)}</span>
              </div>
              <span className="fb-commit-date">{latestCommit.date}</span>
            </div>
          </div>
        )}

        {/* Lignes de fichiers */}
        {entries.length === 0 ? (
          <div className="fb-empty">
            <Folder size={40} />
            <p>Répertoire vide</p>
          </div>
        ) : (
          <div className="fb-list">
            {entries.map((entry) => (
              <button
                key={entry.path}
                className="fb-row"
                onClick={() => handleNavigate(entry)}
                aria-label={`Naviguer vers ${entry.name}`}
              >
                {/* Icône */}
                <div className="fb-row-icon">
                  <EntryIcon entry={entry} />
                </div>
                {/* Nom */}
                <div className="fb-row-name">
                  <span
                    className={
                      entry.kind === "directory" ? "fb-name-dir" : "fb-name-file"
                    }
                  >
                    {entry.name}
                  </span>
                </div>
                {/* Taille (fichiers uniquement) */}
                <div className="fb-row-size">
                  {entry.kind === "file" && (
                    <span className="fb-size">{formatSize(entry.size)}</span>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
