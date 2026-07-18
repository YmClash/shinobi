"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Phase 19B · ImportGitHubForm
// Import a public GitHub repository into the Forge
// ═══════════════════════════════════════════════════════════════

import { useState, useCallback } from "react";
import { getBaseUrl } from "@/lib/api";
import { authHeaders } from "@/lib/auth";

interface GitHubPreview {
  full_name: string;
  name: string;
  description: string | null;
  clone_url: string;
  default_branch: string;
  stars: number;
  forks: number;
  language: string | null;
  license: string | null;
  is_private: boolean;
}

interface ImportGitHubFormProps {
  onImported: () => void;
}

type ImportStep = "input" | "preview" | "importing" | "done" | "error";

export function ImportGitHubForm({ onImported }: ImportGitHubFormProps) {
  const [url, setUrl] = useState("");
  const [nameOverride, setNameOverride] = useState("");
  const [step, setStep] = useState<ImportStep>("input");
  const [preview, setPreview] = useState<GitHubPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [importedRepo, setImportedRepo] = useState<{ id: string; name: string } | null>(null);

  const base = getBaseUrl();

  // ── Preview ─────────────────────────────────────────────────
  const handlePreview = useCallback(async () => {
    if (!url.trim()) return;
    setError(null);
    setStep("preview");

    try {
      const res = await fetch(
        `${base}/api/v1/github/preview?url=${encodeURIComponent(url.trim())}`,
        { headers: authHeaders() }
      );

      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body?.error?.message ?? `Erreur ${res.status}`);
      }

      const data: GitHubPreview = await res.json();

      if (data.is_private) {
        throw new Error("Les dépôts privés ne sont pas supportés (V1).");
      }

      setPreview(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erreur inattendue");
      setStep("error");
    }
  }, [url, base]);

  // ── Import ──────────────────────────────────────────────────
  const handleImport = useCallback(async () => {
    if (!preview) return;
    setError(null);
    setStep("importing");

    try {
      const res = await fetch(`${base}/api/v1/repos/import-github`, {
        method: "POST",
        headers: authHeaders(),
        body: JSON.stringify({
          github_url: url.trim(),
          name_override: nameOverride.trim() || null,
        }),
      });

      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body?.error?.message ?? `Erreur ${res.status}`);
      }

      const repo = await res.json();
      setImportedRepo({ id: repo.id, name: repo.name });
      setStep("done");
      onImported();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erreur inattendue");
      setStep("error");
    }
  }, [preview, url, nameOverride, base, onImported]);

  // ── Reset ───────────────────────────────────────────────────
  const handleReset = useCallback(() => {
    setUrl("");
    setNameOverride("");
    setPreview(null);
    setError(null);
    setImportedRepo(null);
    setStep("input");
  }, []);

  return (
    <div className="import-github-form">
      {/* ── Step 1: URL Input ─────────────────────────── */}
      {(step === "input" || step === "error") && (
        <div className="space-y-4">
          <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
            <span className="w-5 h-5 rounded-full bg-primary/10 text-primary flex items-center justify-center text-[10px] font-bold">1</span>
            <span>Entrez l&apos;URL du dépôt GitHub public à importer</span>
          </div>

          <div className="relative">
            <input
              type="url"
              id="github-import-url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://github.com/owner/repo"
              className="w-full px-3 py-2.5 pl-9 text-sm rounded-lg bg-background border border-border/60 focus:border-primary/50 focus:ring-1 focus:ring-primary/20 outline-none transition-all placeholder:text-muted-foreground/40"
              onKeyDown={(e) => e.key === "Enter" && handlePreview()}
            />
            <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground/40" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
            </svg>
          </div>

          {error && (
            <div className="text-xs text-destructive bg-destructive/10 rounded-lg px-3 py-2 border border-destructive/20">
              {error}
            </div>
          )}

          <button
            onClick={handlePreview}
            disabled={!url.trim()}
            className="w-full px-4 py-2 text-xs font-medium rounded-lg bg-[hsl(210,80%,50%)] text-white hover:bg-[hsl(210,80%,45%)] disabled:opacity-40 disabled:cursor-not-allowed transition-all cursor-pointer hover:shadow-lg hover:shadow-[hsl(210,80%,50%)]/20"
          >
            🔍 Prévisualiser le dépôt
          </button>
        </div>
      )}

      {/* ── Step 2: Preview ───────────────────────────── */}
      {step === "preview" && preview && (
        <div className="space-y-4 animate-fade-in-up">
          <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
            <span className="w-5 h-5 rounded-full bg-primary/10 text-primary flex items-center justify-center text-[10px] font-bold">2</span>
            <span>Confirmez l&apos;import</span>
          </div>

          {/* Preview Card */}
          <div className="rounded-lg border border-border/40 bg-card/50 p-4 space-y-3">
            <div className="flex items-start justify-between">
              <div>
                <h3 className="text-sm font-semibold text-foreground">{preview.full_name}</h3>
                {preview.description && (
                  <p className="text-xs text-muted-foreground mt-1 line-clamp-2">{preview.description}</p>
                )}
              </div>
              <span className="shrink-0 text-xs font-mono px-2 py-0.5 rounded bg-[hsl(210,80%,50%)]/10 text-[hsl(210,80%,50%)] border border-[hsl(210,80%,50%)]/20">
                GitHub
              </span>
            </div>

            <div className="flex items-center gap-4 text-xs text-muted-foreground">
              {preview.language && (
                <span className="flex items-center gap-1">
                  <span className="w-2 h-2 rounded-full bg-[hsl(45,90%,50%)]" />
                  {preview.language}
                </span>
              )}
              <span>⭐ {preview.stars.toLocaleString()}</span>
              <span>🔀 {preview.forks.toLocaleString()}</span>
              {preview.license && <span>📄 {preview.license}</span>}
              <span className="text-[10px] opacity-60">🌿 {preview.default_branch}</span>
            </div>
          </div>

          {/* Name Override */}
          <div>
            <label htmlFor="github-import-name" className="text-xs text-muted-foreground mb-1 block">
              Nom dans SHINOBI (optionnel — défaut : {preview.name})
            </label>
            <input
              type="text"
              id="github-import-name"
              value={nameOverride}
              onChange={(e) => setNameOverride(e.target.value)}
              placeholder={preview.name}
              className="w-full px-3 py-2 text-sm rounded-lg bg-background border border-border/60 focus:border-primary/50 focus:ring-1 focus:ring-primary/20 outline-none transition-all placeholder:text-muted-foreground/30"
            />
          </div>

          <div className="flex gap-2">
            <button
              onClick={handleReset}
              className="px-4 py-2 text-xs font-medium rounded-lg bg-muted text-muted-foreground hover:bg-muted/80 transition-all cursor-pointer"
            >
              ← Retour
            </button>
            <button
              onClick={handleImport}
              className="flex-1 px-4 py-2 text-xs font-medium rounded-lg bg-emerald-600 text-white hover:bg-emerald-500 transition-all cursor-pointer hover:shadow-lg hover:shadow-emerald-600/20"
            >
              🌉 Importer dans SHINOBI
            </button>
          </div>
        </div>
      )}

      {/* ── Step 2b: Preview Loading ──────────────────── */}
      {step === "preview" && !preview && (
        <div className="flex items-center justify-center py-8">
          <div className="import-github-spinner" />
          <span className="text-xs text-muted-foreground ml-3">Interrogation de l&apos;API GitHub...</span>
        </div>
      )}

      {/* ── Step 3: Importing ─────────────────────────── */}
      {step === "importing" && (
        <div className="flex flex-col items-center justify-center py-8 space-y-3">
          <div className="import-github-spinner" />
          <p className="text-xs text-muted-foreground">Import en cours — Le Fetch Injecté...</p>
          <p className="text-[10px] text-muted-foreground/60">
            Aspiration des refs depuis GitHub + synchronisation Jujutsu
          </p>
        </div>
      )}

      {/* ── Step 4: Done ──────────────────────────────── */}
      {step === "done" && importedRepo && (
        <div className="flex flex-col items-center justify-center py-8 space-y-3 animate-fade-in-up">
          <span className="text-4xl">🌉</span>
          <p className="text-sm font-semibold text-emerald-500">Import réussi !</p>
          <p className="text-xs text-muted-foreground">
            Le dépôt <strong>{importedRepo.name}</strong> est maintenant disponible dans la Forge.
          </p>
          <button
            onClick={handleReset}
            className="mt-2 px-4 py-2 text-xs font-medium rounded-lg bg-muted text-muted-foreground hover:bg-muted/80 transition-all cursor-pointer"
          >
            Importer un autre dépôt
          </button>
        </div>
      )}
    </div>
  );
}
