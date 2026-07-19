"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Phase 20B · GitHubReposList
// 🐙 Le Clonage Massif — List, select & bulk-import GitHub repos
// ═══════════════════════════════════════════════════════════════

import { useState, useCallback, useEffect } from "react";
import {
  fetchGitHubRepos,
  bulkImportGitHub,
  type GitHubRepoWithStatus,
  type BulkImportResult,
} from "@/lib/auth";

interface GitHubReposListProps {
  onImported: () => void;
}

type Phase = "loading" | "list" | "importing" | "done" | "error";

// ── Language color dots ─────────────────────────────────────
const LANG_COLORS: Record<string, string> = {
  Rust: "hsl(15, 85%, 55%)",
  TypeScript: "hsl(210, 80%, 50%)",
  JavaScript: "hsl(45, 90%, 50%)",
  Python: "hsl(210, 60%, 45%)",
  Go: "hsl(190, 70%, 45%)",
  Java: "hsl(20, 70%, 50%)",
  "C++": "hsl(340, 70%, 55%)",
  C: "hsl(210, 50%, 40%)",
  Ruby: "hsl(0, 70%, 50%)",
  Swift: "hsl(15, 90%, 55%)",
  Kotlin: "hsl(270, 60%, 55%)",
  Shell: "hsl(120, 50%, 40%)",
  HTML: "hsl(15, 80%, 55%)",
  CSS: "hsl(270, 50%, 55%)",
  Dart: "hsl(200, 80%, 50%)",
};

function getLangColor(lang: string | null): string {
  if (!lang) return "hsl(0, 0%, 50%)";
  return LANG_COLORS[lang] ?? "hsl(0, 0%, 50%)";
}

export function GitHubReposList({ onImported }: GitHubReposListProps) {
  const [phase, setPhase] = useState<Phase>("loading");
  const [repos, setRepos] = useState<GitHubRepoWithStatus[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<BulkImportResult | null>(null);
  const [importProgress, setImportProgress] = useState(0);

  // ── Load repos ─────────────────────────────────────────
  const loadRepos = useCallback(async () => {
    setPhase("loading");
    setError(null);
    try {
      const data = await fetchGitHubRepos();
      setRepos(data.repos);
      setPhase("list");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erreur inattendue");
      setPhase("error");
    }
  }, []);

  useEffect(() => {
    loadRepos();
  }, [loadRepos]);

  // ── Toggle selection ───────────────────────────────────
  const toggleRepo = useCallback((cloneUrl: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(cloneUrl)) {
        next.delete(cloneUrl);
      } else {
        next.add(cloneUrl);
      }
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    const importable = repos
      .filter((r) => !r.already_imported)
      .map((r) => r.clone_url);
    setSelected(new Set(importable));
  }, [repos]);

  const deselectAll = useCallback(() => {
    setSelected(new Set());
  }, []);

  // ── Bulk import ────────────────────────────────────────
  const handleBulkImport = useCallback(async () => {
    if (selected.size === 0) return;
    setPhase("importing");
    setImportProgress(0);
    setError(null);

    try {
      // We can't track per-repo progress with a single POST,
      // but we animate the progress bar smoothly
      const progressInterval = setInterval(() => {
        setImportProgress((prev) => {
          const target = 90; // Don't go to 100 until done
          return prev < target ? prev + (target - prev) * 0.05 : prev;
        });
      }, 500);

      const result = await bulkImportGitHub(Array.from(selected));
      clearInterval(progressInterval);
      setImportProgress(100);
      setImportResult(result);
      setPhase("done");
      onImported();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erreur d'import");
      setPhase("error");
    }
  }, [selected, onImported]);

  // ── Reset ──────────────────────────────────────────────
  const handleReset = useCallback(() => {
    setSelected(new Set());
    setImportResult(null);
    setImportProgress(0);
    loadRepos();
  }, [loadRepos]);

  const importableCount = repos.filter((r) => !r.already_imported).length;

  // ─────────────────────────────────────────────────────────
  //  RENDER
  // ─────────────────────────────────────────────────────────

  // ── Loading ────────────────────────────────────────────
  if (phase === "loading") {
    return (
      <div className="gh-loading">
        <div className="gh-spinner" />
        <span className="text-xs text-muted-foreground ml-3">
          Chargement de vos repos GitHub...
        </span>
      </div>
    );
  }

  // ── Error ──────────────────────────────────────────────
  if (phase === "error") {
    return (
      <div className="space-y-3">
        <div className="text-xs text-destructive bg-destructive/10 rounded-lg px-3 py-2 border border-destructive/20">
          {error}
        </div>
        <button
          onClick={loadRepos}
          className="px-4 py-2 text-xs font-medium rounded-lg bg-muted text-muted-foreground hover:bg-muted/80 transition-all cursor-pointer"
        >
          🔄 Réessayer
        </button>
      </div>
    );
  }

  // ── Importing ──────────────────────────────────────────
  if (phase === "importing") {
    return (
      <div className="gh-importing">
        <span className="text-4xl mb-3">🐙</span>
        <p className="text-sm font-semibold text-foreground">Le Clonage Massif en cours...</p>
        <p className="text-[10px] text-muted-foreground/60 mt-1">
          Import séquentiel — {selected.size} dépôt{selected.size > 1 ? "s" : ""}
        </p>
        <div className="gh-progress-bar mt-4">
          <div
            className="gh-progress-fill"
            style={{ width: `${importProgress}%` }}
          />
        </div>
        <p className="text-[10px] text-muted-foreground/40 mt-2">
          Aspiration des refs + synchronisation Jujutsu...
        </p>
      </div>
    );
  }

  // ── Done ───────────────────────────────────────────────
  if (phase === "done" && importResult) {
    return (
      <div className="gh-done animate-fade-in-up">
        <span className="text-4xl mb-3">🎉</span>
        <p className="text-sm font-semibold text-emerald-500">Clonage Massif terminé !</p>

        <div className="gh-done-stats">
          {importResult.imported > 0 && (
            <span className="gh-stat gh-stat-ok">
              ✅ {importResult.imported} importé{importResult.imported > 1 ? "s" : ""}
            </span>
          )}
          {importResult.skipped > 0 && (
            <span className="gh-stat gh-stat-skip">
              ⏭️ {importResult.skipped} skippé{importResult.skipped > 1 ? "s" : ""}
            </span>
          )}
          {importResult.failed > 0 && (
            <span className="gh-stat gh-stat-err">
              ❌ {importResult.failed} erreur{importResult.failed > 1 ? "s" : ""}
            </span>
          )}
        </div>

        {/* Per-repo details */}
        <div className="gh-done-details">
          {importResult.results.map((item) => (
            <div
              key={item.github_url}
              className={`gh-done-item gh-done-item-${item.status}`}
            >
              <span className="gh-done-icon">
                {item.status === "imported" ? "✅" : item.status === "skipped" ? "⏭️" : "❌"}
              </span>
              <span className="gh-done-name">{item.shinobi_name ?? item.github_url.split("/").pop()}</span>
              {item.message && (
                <span className="gh-done-msg">{item.message}</span>
              )}
            </div>
          ))}
        </div>

        <button
          onClick={handleReset}
          className="mt-4 px-4 py-2 text-xs font-medium rounded-lg bg-muted text-muted-foreground hover:bg-muted/80 transition-all cursor-pointer"
        >
          ← Retour à la liste
        </button>
      </div>
    );
  }

  // ── List (main view) ───────────────────────────────────
  return (
    <div className="gh-repos-container">
      {/* Header */}
      <div className="gh-repos-header">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="w-5 h-5 rounded-full bg-[hsl(210,80%,50%)]/10 text-[hsl(210,80%,50%)] flex items-center justify-center text-[10px] font-bold">
            🐙
          </span>
          <span>
            {repos.length} dépôt{repos.length > 1 ? "s" : ""} publics trouvés
            {repos.length - importableCount > 0 && (
              <> · {repos.length - importableCount} déjà importé{repos.length - importableCount > 1 ? "s" : ""}</>
            )}
          </span>
        </div>

        <div className="flex items-center gap-2">
          {importableCount > 0 && (
            <button
              onClick={selected.size === importableCount ? deselectAll : selectAll}
              className="text-[10px] text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
            >
              {selected.size === importableCount ? "Tout décocher" : "Tout cocher"}
            </button>
          )}
        </div>
      </div>

      {/* Repos grid */}
      <div className="gh-repos-grid">
        {repos.map((repo) => {
          const isImported = repo.already_imported;
          const isSelected = selected.has(repo.clone_url);

          return (
            <button
              key={repo.clone_url}
              onClick={() => !isImported && toggleRepo(repo.clone_url)}
              disabled={isImported}
              className={`gh-repo-card ${isImported ? "gh-repo-imported" : ""} ${
                isSelected ? "gh-repo-selected" : ""
              }`}
            >
              {/* Checkbox */}
              <div className={`gh-repo-check ${isSelected ? "gh-repo-check-on" : ""} ${isImported ? "gh-repo-check-off" : ""}`}>
                {isSelected && "✓"}
                {isImported && "✓"}
              </div>

              {/* Info */}
              <div className="gh-repo-info">
                <div className="gh-repo-name">
                  {repo.name}
                  {isImported && (
                    <span className="gh-repo-badge">Importé</span>
                  )}
                </div>
                {repo.description && (
                  <p className="gh-repo-desc">{repo.description}</p>
                )}
                <div className="gh-repo-meta">
                  {repo.language && (
                    <span className="gh-repo-lang">
                      <span
                        className="gh-repo-dot"
                        style={{ backgroundColor: getLangColor(repo.language) }}
                      />
                      {repo.language}
                    </span>
                  )}
                  <span>⭐ {repo.stars.toLocaleString()}</span>
                  <span>🔀 {repo.forks.toLocaleString()}</span>
                  {repo.license && <span>📄 {repo.license}</span>}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      {/* Empty state */}
      {repos.length === 0 && (
        <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
          <span className="text-4xl mb-3 opacity-30">🐙</span>
          <p className="text-xs">Aucun dépôt public trouvé sur votre compte GitHub</p>
        </div>
      )}

      {/* Import button */}
      {selected.size > 0 && (
        <div className="gh-import-bar animate-fade-in-up">
          <span className="text-xs text-foreground">
            <strong>{selected.size}</strong> dépôt{selected.size > 1 ? "s" : ""} sélectionné{selected.size > 1 ? "s" : ""}
          </span>
          <button
            onClick={handleBulkImport}
            className="gh-import-btn"
          >
            🐙 Importer dans SHINOBI
          </button>
        </div>
      )}
    </div>
  );
}
