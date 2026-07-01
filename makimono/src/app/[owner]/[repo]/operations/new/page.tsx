"use client";

import { useState, useCallback, useRef } from "react";
import { useRouter, useParams } from "next/navigation";
import Link from "next/link";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { forgeOperation } from "./actions";

// ── Language detection ───────────────────────────────

const LANG_CONFIG: Record<string, { color: string; icon: string; label: string }> = {
  rs:   { color: "bg-orange-500/15 text-orange-400 border-orange-500/30", icon: "🦀", label: "Rust" },
  ts:   { color: "bg-blue-500/15 text-blue-400 border-blue-500/30", icon: "TS", label: "TypeScript" },
  tsx:  { color: "bg-blue-500/15 text-blue-400 border-blue-500/30", icon: "⚛", label: "TSX" },
  css:  { color: "bg-pink-500/15 text-pink-400 border-pink-500/30", icon: "🎨", label: "CSS" },
  py:   { color: "bg-yellow-500/15 text-yellow-400 border-yellow-500/30", icon: "🐍", label: "Python" },
  js:   { color: "bg-yellow-500/15 text-yellow-400 border-yellow-500/30", icon: "JS", label: "JavaScript" },
  jsx:  { color: "bg-cyan-500/15 text-cyan-400 border-cyan-500/30", icon: "⚛", label: "JSX" },
  json: { color: "bg-gray-500/15 text-gray-400 border-gray-500/30", icon: "{}", label: "JSON" },
  toml: { color: "bg-gray-500/15 text-gray-400 border-gray-500/30", icon: "⚙", label: "TOML" },
  yaml: { color: "bg-purple-500/15 text-purple-400 border-purple-500/30", icon: "📋", label: "YAML" },
  yml:  { color: "bg-purple-500/15 text-purple-400 border-purple-500/30", icon: "📋", label: "YAML" },
  md:   { color: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30", icon: "📝", label: "Markdown" },
  html: { color: "bg-red-500/15 text-red-400 border-red-500/30", icon: "🌐", label: "HTML" },
  sql:  { color: "bg-indigo-500/15 text-indigo-400 border-indigo-500/30", icon: "🗄", label: "SQL" },
};

const ACCEPTED_EXTENSIONS = Object.keys(LANG_CONFIG).map((ext) => `.${ext}`).join(",");

function getExtension(name: string): string {
  return name.split(".").pop()?.toLowerCase() ?? "";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// ── Staged file type ─────────────────────────────────

interface StagedFile {
  file: File;
  id: string;
}

// ── Page ─────────────────────────────────────────────

export default function NewOperationPage() {
  const router = useRouter();
  const params = useParams();
  const owner = params.owner as string;
  const repo = params.repo as string;

  const formRef = useRef<HTMLFormElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [description, setDescription] = useState("");
  const [stagedFiles, setStagedFiles] = useState<StagedFile[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [isForging, setIsForging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [forgeSuccess, setForgeSuccess] = useState(false);

  // ── File handlers ──────────────────────────────────

  const addFiles = useCallback((files: FileList | File[]) => {
    const newFiles: StagedFile[] = [];
    for (const file of Array.from(files)) {
      const ext = getExtension(file.name);
      if (!LANG_CONFIG[ext]) continue; // Skip unsupported
      if (file.size > 5 * 1024 * 1024) continue; // 5MB limit
      newFiles.push({ file, id: `${file.name}-${Date.now()}-${Math.random()}` });
    }
    setStagedFiles((prev) => [...prev, ...newFiles]);
    setError(null);
  }, []);

  const removeFile = useCallback((id: string) => {
    setStagedFiles((prev) => prev.filter((f) => f.id !== id));
  }, []);

  // ── Drag & Drop handlers ───────────────────────────

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(false);
      if (e.dataTransfer.files.length > 0) {
        addFiles(e.dataTransfer.files);
      }
    },
    [addFiles],
  );

  // ── Submit handler ─────────────────────────────────

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setError(null);

      if (!description.trim()) {
        setError("La description de l'opération est requise.");
        return;
      }
      if (stagedFiles.length === 0) {
        setError("Glissez au moins un fichier dans la zone de drop.");
        return;
      }

      setIsForging(true);

      try {
        // Build FormData for the Server Action
        const formData = new FormData();
        formData.set("description", description.trim());
        formData.set("owner", owner);
        formData.set("repo", repo);
        for (const staged of stagedFiles) {
          formData.append("files", staged.file);
        }

        const result = await forgeOperation(formData);

        if (!result.success) {
          setError(result.error ?? "Erreur inconnue");
          setIsForging(false);
          return;
        }

        // Success animation then redirect
        setForgeSuccess(true);
        setTimeout(() => {
          router.push(`/${owner}/${repo}/operations`);
        }, 1500);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Erreur inattendue");
        setIsForging(false);
      }
    },
    [description, stagedFiles, router, owner, repo],
  );

  // ── Render ─────────────────────────────────────────

  const totalSize = stagedFiles.reduce((sum, f) => sum + f.file.size, 0);

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-bold tracking-wide">
            🏯 Salle des Commandes
          </h1>
          <p className="text-xs text-muted-foreground mt-1">
            Forgez une nouvelle opération VCS dans <code className="font-mono bg-muted px-1 py-0.5 rounded text-[10px]">{owner}/{repo}</code> — le pipeline complet s&apos;exécutera automatiquement
          </p>
        </div>
        <Link
          href={`/${owner}/${repo}/operations`}
          className="text-xs text-muted-foreground hover:text-foreground transition-colors"
        >
          ← Retour au DAG
        </Link>
      </div>

      {/* Success overlay */}
      {forgeSuccess && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm">
          <div className="text-center space-y-4 animate-fade-in-up">
            <div className="text-7xl">⚔️</div>
            <h2 className="text-xl font-bold">Opération Forgée !</h2>
            <p className="text-sm text-muted-foreground">
              Le pipeline s&apos;exécute en arrière-plan...
            </p>
            <div className="flex items-center gap-2 justify-center text-xs text-muted-foreground">
              <span className="animate-pulse">🌳 jj</span>
              <span>→</span>
              <span className="animate-pulse" style={{ animationDelay: "0.2s" }}>🌐 IPFS</span>
              <span>→</span>
              <span className="animate-pulse" style={{ animationDelay: "0.4s" }}>📨 Kafka</span>
              <span>→</span>
              <span className="animate-pulse" style={{ animationDelay: "0.6s" }}>🧠 Tensai</span>
              <span>→</span>
              <span className="animate-pulse" style={{ animationDelay: "0.8s" }}>🧬 pgvector</span>
            </div>
          </div>
        </div>
      )}

      <form ref={formRef} onSubmit={handleSubmit} className="space-y-5">
        {/* Description field */}
        <Card className="glass-card neon-glow">
          <CardContent className="p-4 space-y-2">
            <label
              htmlFor="op-description"
              className="text-xs font-medium text-muted-foreground flex items-center gap-1.5"
            >
              📋 Description de l&apos;opération
            </label>
            <textarea
              id="op-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Décrivez votre changement... ex: Ajout du module d'authentification avec JWT et refresh tokens"
              rows={3}
              className="w-full bg-muted/30 border border-border/50 rounded-lg px-3 py-2.5 text-sm
                         placeholder:text-muted-foreground/40 focus:outline-none focus:ring-2
                         focus:ring-primary/30 focus:border-primary/50 resize-none transition-all"
            />
            <p className="text-[10px] text-muted-foreground/60">
              Ce message apparaîtra dans le graphe DAG et l&apos;historique VCS.
            </p>
          </CardContent>
        </Card>

        {/* Drop zone */}
        <Card
          className={`glass-card transition-all duration-300 cursor-pointer ${
            isDragging
              ? "border-primary border-2 bg-primary/5 scale-[1.01] shadow-lg shadow-primary/10"
              : "border-dashed border-2 border-border/50 hover:border-primary/30"
          }`}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
          onClick={() => fileInputRef.current?.click()}
        >
          <CardContent className="p-8 flex flex-col items-center justify-center text-center">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              accept={ACCEPTED_EXTENSIONS}
              className="hidden"
              onChange={(e) => {
                if (e.target.files) addFiles(e.target.files);
                e.target.value = ""; // Reset for re-selection
              }}
            />

            <div
              className={`text-5xl mb-3 transition-transform duration-300 ${
                isDragging ? "scale-125 animate-bounce" : ""
              }`}
            >
              {isDragging ? "🎯" : "📂"}
            </div>

            <p className="text-sm font-medium">
              {isDragging
                ? "Déposez vos fichiers ici !"
                : "Glissez vos fichiers ici ou cliquez pour sélectionner"}
            </p>
            <p className="text-[10px] text-muted-foreground/60 mt-2">
              Rust · TypeScript · TSX · CSS · Python · JSON · TOML · YAML · Markdown · HTML · SQL
            </p>
            <p className="text-[10px] text-muted-foreground/40 mt-1">
              Limite : 5 MB par fichier
            </p>
          </CardContent>
        </Card>

        {/* Staged files list */}
        {stagedFiles.length > 0 && (
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-muted-foreground">
                📦 Fichiers stagés ({stagedFiles.length})
              </span>
              <span className="text-[10px] text-muted-foreground/60">
                Total : {formatBytes(totalSize)}
              </span>
            </div>

            <div className="grid gap-1.5">
              {stagedFiles.map((staged, i) => {
                const ext = getExtension(staged.file.name);
                const config = LANG_CONFIG[ext] ?? {
                  color: "bg-muted text-muted-foreground",
                  icon: "📄",
                  label: ext,
                };

                return (
                  <div
                    key={staged.id}
                    className="flex items-center gap-2 px-3 py-2 rounded-lg bg-muted/30
                               border border-border/30 group hover:border-border/60
                               transition-all animate-fade-in-up"
                    style={{ animationDelay: `${i * 50}ms` }}
                  >
                    <Badge
                      variant="outline"
                      className={`text-[10px] font-mono px-1.5 py-0 shrink-0 ${config.color}`}
                    >
                      {config.icon}
                    </Badge>
                    <span className="font-mono text-xs flex-1 truncate">
                      {staged.file.name}
                    </span>
                    <span className="text-[10px] text-muted-foreground/60 shrink-0">
                      {formatBytes(staged.file.size)}
                    </span>
                    <Badge variant="secondary" className="text-[10px] px-1.5 py-0 shrink-0">
                      {config.label}
                    </Badge>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        removeFile(staged.id);
                      }}
                      className="text-muted-foreground/40 hover:text-destructive transition-colors
                                 opacity-0 group-hover:opacity-100 text-xs shrink-0 cursor-pointer"
                      aria-label={`Retirer ${staged.file.name}`}
                    >
                      ✕
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Error display */}
        {error && (
          <div className="text-sm text-destructive bg-destructive/10 rounded-lg p-3 border border-destructive/20 animate-fade-in-up">
            ⚠️ {error}
          </div>
        )}

        {/* Submit button */}
        <button
          type="submit"
          disabled={isForging || stagedFiles.length === 0 || !description.trim()}
          className={`w-full py-3 px-6 rounded-lg font-medium text-sm transition-all duration-300
                     flex items-center justify-center gap-2 cursor-pointer
                     ${
                       isForging
                         ? "bg-primary/50 text-primary-foreground/70 cursor-wait"
                         : stagedFiles.length === 0 || !description.trim()
                           ? "bg-muted text-muted-foreground cursor-not-allowed opacity-50"
                           : "bg-primary text-primary-foreground hover:bg-primary/90 hover:shadow-lg hover:shadow-primary/20 hover:scale-[1.01] active:scale-[0.99]"
                     }`}
        >
          {isForging ? (
            <>
              <span className="animate-spin">⚙️</span>
              <span>Forge en cours...</span>
            </>
          ) : (
            <>
              <span>⚔️</span>
              <span>Forger l&apos;Opération</span>
              {stagedFiles.length > 0 && (
                <Badge variant="secondary" className="text-[10px] ml-1">
                  {stagedFiles.length} fichier{stagedFiles.length > 1 ? "s" : ""}
                </Badge>
              )}
            </>
          )}
        </button>

        {/* Pipeline hint */}
        <div className="flex items-center justify-center gap-2 text-[10px] text-muted-foreground/40">
          <span>🌳 jj-lib</span>
          <span>→</span>
          <span>🌐 IPFS</span>
          <span>→</span>
          <span>📨 Kafka</span>
          <span>→</span>
          <span>🧠 Tensai</span>
          <span>→</span>
          <span>🧬 pgvector</span>
        </div>
      </form>
    </div>
  );
}
