"use client";

import { useState, useCallback } from "react";
import { Badge } from "@/components/ui/badge";

// ═══════════════════════════════════════════════════════════════
// CreateRepoForm — Inline form for creating a new repository
// ═══════════════════════════════════════════════════════════════

interface CreateRepoFormProps {
  onCreated: () => void;
  forgeAction: (formData: FormData) => Promise<{ success: boolean; error?: string }>;
}

/** Validates a slug: lowercase alphanumeric + hyphens, 3-64 chars */
function isValidSlug(s: string): boolean {
  return /^[a-z0-9][a-z0-9-]{1,62}[a-z0-9]$/.test(s) && !s.includes("--");
}

/** Converts a display name to a slug preview */
function toSlug(s: string): string {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 64);
}

export function CreateRepoForm({ onCreated, forgeAction }: CreateRepoFormProps) {
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState<"public" | "private">("public");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [autoSlug, setAutoSlug] = useState(true);

  // Auto-derive slug from display name
  const handleDisplayNameChange = useCallback(
    (value: string) => {
      setDisplayName(value);
      if (autoSlug) {
        setName(toSlug(value));
      }
    },
    [autoSlug],
  );

  const handleNameChange = useCallback((value: string) => {
    setName(value.toLowerCase().replace(/[^a-z0-9-]/g, ""));
    setAutoSlug(false);
  }, []);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setError(null);

      if (!displayName.trim()) {
        setError("Le nom d'affichage est requis.");
        return;
      }

      if (!name || !isValidSlug(name)) {
        setError("Le slug doit contenir 3-64 caractères (a-z, 0-9, tirets), sans double-tiret.");
        return;
      }

      setIsSubmitting(true);

      try {
        const formData = new FormData();
        formData.set("name", name);
        formData.set("display_name", displayName.trim());
        formData.set("description", description.trim());
        formData.set("visibility", visibility);

        const result = await forgeAction(formData);

        if (!result.success) {
          setError(result.error ?? "Erreur inconnue");
          setIsSubmitting(false);
          return;
        }

        // Reset form
        setName("");
        setDisplayName("");
        setDescription("");
        setVisibility("public");
        setAutoSlug(true);
        setIsSubmitting(false);

        onCreated();
      } catch (err) {
        setError(err instanceof Error ? err.message : "Erreur inattendue");
        setIsSubmitting(false);
      }
    },
    [name, displayName, description, visibility, forgeAction, onCreated],
  );

  const slugValid = name.length === 0 || isValidSlug(name);

  return (
    <div className="forge-create-section">
      <form onSubmit={handleSubmit} className="space-y-4">
        {/* Display Name */}
        <div className="space-y-1.5">
          <label htmlFor="repo-display-name" className="text-xs font-medium text-muted-foreground">
            Nom d&apos;affichage
          </label>
          <input
            id="repo-display-name"
            type="text"
            value={displayName}
            onChange={(e) => handleDisplayNameChange(e.target.value)}
            placeholder="Mon Super Projet"
            maxLength={128}
            className="w-full bg-muted/30 border border-border/50 rounded-lg px-3 py-2 text-sm
                       placeholder:text-muted-foreground/40 focus:outline-none focus:ring-2
                       focus:ring-primary/30 focus:border-primary/50 transition-all"
          />
        </div>

        {/* Slug */}
        <div className="space-y-1.5">
          <label htmlFor="repo-slug" className="text-xs font-medium text-muted-foreground flex items-center gap-2">
            Identifiant (slug)
            {name && (
              <span className={`text-[10px] font-mono ${slugValid ? "text-emerald-500" : "text-destructive"}`}>
                system/{name}
              </span>
            )}
          </label>
          <input
            id="repo-slug"
            type="text"
            value={name}
            onChange={(e) => handleNameChange(e.target.value)}
            placeholder="mon-super-projet"
            maxLength={64}
            className={`w-full bg-muted/30 border rounded-lg px-3 py-2 text-sm font-mono
                       placeholder:text-muted-foreground/40 focus:outline-none focus:ring-2
                       transition-all
                       ${
                         name && !slugValid
                           ? "border-destructive/50 focus:ring-destructive/30"
                           : "border-border/50 focus:ring-primary/30 focus:border-primary/50"
                       }`}
          />
          <p className="text-[10px] text-muted-foreground/50">
            Lettres minuscules, chiffres et tirets. Min 3, max 64 caractères.
          </p>
        </div>

        {/* Description */}
        <div className="space-y-1.5">
          <label htmlFor="repo-description" className="text-xs font-medium text-muted-foreground">
            Description <span className="text-muted-foreground/40">(optionnel)</span>
          </label>
          <textarea
            id="repo-description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Une courte description de votre dépôt..."
            rows={2}
            maxLength={500}
            className="w-full bg-muted/30 border border-border/50 rounded-lg px-3 py-2 text-sm
                       placeholder:text-muted-foreground/40 focus:outline-none focus:ring-2
                       focus:ring-primary/30 focus:border-primary/50 resize-none transition-all"
          />
        </div>

        {/* Visibility Toggle */}
        <div className="space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">Visibilité</span>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setVisibility("public")}
              className={`forge-visibility-toggle ${
                visibility === "public"
                  ? "bg-emerald-500/15 text-emerald-500 border-emerald-500/40"
                  : "bg-muted/30 text-muted-foreground border-border/50 hover:border-border"
              }`}
            >
              🌍 Public
            </button>
            <button
              type="button"
              onClick={() => setVisibility("private")}
              className={`forge-visibility-toggle ${
                visibility === "private"
                  ? "bg-amber-500/15 text-amber-500 border-amber-500/40"
                  : "bg-muted/30 text-muted-foreground border-border/50 hover:border-border"
              }`}
            >
              🔒 Privé
            </button>
          </div>
        </div>

        {/* Error */}
        {error && (
          <div className="text-sm text-destructive bg-destructive/10 rounded-lg p-3 border border-destructive/20 animate-fade-in-up">
            ⚠️ {error}
          </div>
        )}

        {/* Submit */}
        <button
          type="submit"
          disabled={isSubmitting || !displayName.trim() || !slugValid || name.length === 0}
          className={`w-full py-2.5 px-4 rounded-lg font-medium text-sm transition-all duration-300
                     flex items-center justify-center gap-2 cursor-pointer
                     ${
                       isSubmitting
                         ? "bg-primary/50 text-primary-foreground/70 cursor-wait"
                         : !displayName.trim() || !slugValid || name.length === 0
                           ? "bg-muted text-muted-foreground cursor-not-allowed opacity-50"
                           : "bg-primary text-primary-foreground hover:bg-primary/90 hover:shadow-lg hover:shadow-primary/20 hover:scale-[1.01] active:scale-[0.99]"
                     }`}
        >
          {isSubmitting ? (
            <>
              <span className="animate-spin">⚙️</span>
              <span>Forge en cours...</span>
            </>
          ) : (
            <>
              <span>🔨</span>
              <span>Forger le Dépôt</span>
            </>
          )}
        </button>
      </form>
    </div>
  );
}
