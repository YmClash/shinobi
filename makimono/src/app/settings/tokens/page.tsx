"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/hooks/use-auth";
import { createPat, listPats, type PatInfo } from "@/lib/auth";
import { Separator } from "@/components/ui/separator";

export default function TokensPage() {
  const router = useRouter();
  const { user, loading: authLoading } = useAuth();
  const [pats, setPats] = useState<PatInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [label, setLabel] = useState("");
  const [newToken, setNewToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  // Redirect if not authenticated
  useEffect(() => {
    if (!authLoading && !user) {
      router.push("/login");
    }
  }, [authLoading, user, router]);

  // Load tokens
  const loadTokens = useCallback(async () => {
    try {
      const res = await listPats();
      setPats(res.tokens);
    } catch {
      setError("Impossible de charger vos tokens");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (user) loadTokens();
  }, [user, loadTokens]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setNewToken(null);
    setCreating(true);
    try {
      const res = await createPat(label || undefined);
      setNewToken(res.token);
      setLabel("");
      await loadTokens();
    } catch (err: unknown) {
      setError(
        err instanceof Error ? err.message : "Erreur de création du token"
      );
    } finally {
      setCreating(false);
    }
  };

  const copyToken = async () => {
    if (newToken) {
      await navigator.clipboard.writeText(newToken);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  if (authLoading || !user) {
    return (
      <div className="auth-page">
        <div className="auth-card">
          <div className="auth-spinner" />
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-2xl mx-auto space-y-8">
      {/* ── Header ────────────────────────── */}
      <div>
        <h1 className="text-lg font-bold tracking-tight">
          🔑 Personal Access Tokens
        </h1>
        <p className="text-sm text-muted-foreground mt-1">
          Les PAT sont utilisés pour l&apos;authentification Git (
          <code className="text-xs font-mono bg-muted px-1.5 py-0.5 rounded">
            git push
          </code>
          ). Chaque token hérite de vos droits RBAC.
        </p>
      </div>

      <Separator />

      {/* ── Create Token ──────────────────── */}
      <div className="pat-create-section">
        <h2 className="text-sm font-semibold mb-3">Créer un nouveau token</h2>

        {error && (
          <div className="auth-error mb-3">
            <span>⚠️</span>
            <span>{error}</span>
          </div>
        )}

        {/* New token reveal */}
        {newToken && (
          <div className="pat-reveal animate-fade-in-up">
            <div className="pat-reveal-header">
              <span className="text-green-500">✅</span>
              <span className="text-sm font-medium">
                Token créé avec succès
              </span>
            </div>
            <p className="text-xs text-muted-foreground mb-2">
              ⚠️ Ce token ne sera plus jamais affiché. Copiez-le maintenant !
            </p>
            <div className="pat-token-display">
              <code className="pat-token-code">{newToken}</code>
              <button
                onClick={copyToken}
                className="pat-copy-btn"
                title="Copier le token"
              >
                {copied ? "✅" : "📋"}
              </button>
            </div>
          </div>
        )}

        <form onSubmit={handleCreate} className="pat-create-form">
          <input
            type="text"
            placeholder="Label (optionnel, ex: laptop-dev)"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            className="auth-input"
            disabled={creating}
          />
          <button
            type="submit"
            disabled={creating}
            className="auth-submit pat-create-btn"
          >
            {creating ? (
              <>
                <span className="auth-spinner" /> Création...
              </>
            ) : (
              "Créer le token"
            )}
          </button>
        </form>
      </div>

      <Separator />

      {/* ── Token List ────────────────────── */}
      <div>
        <h2 className="text-sm font-semibold mb-3">
          Tokens actifs ({pats.length})
        </h2>

        {loading ? (
          <div className="space-y-2">
            {[...Array(2)].map((_, i) => (
              <div
                key={i}
                className="h-12 rounded-lg bg-muted animate-pulse"
              />
            ))}
          </div>
        ) : pats.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground">
            <span className="text-3xl block mb-2 opacity-30">🔑</span>
            <p className="text-sm">Aucun token actif</p>
            <p className="text-xs mt-1 opacity-60">
              Créez un token pour utiliser git push avec SHINOBI
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {pats.map((pat) => (
              <div key={pat.id} className="pat-item">
                <div className="pat-item-info">
                  <span className="pat-item-icon">🔑</span>
                  <span className="pat-item-label">
                    {pat.label || "Token sans label"}
                  </span>
                </div>
                <span className="pat-item-date">
                  {new Date(pat.created_at).toLocaleDateString("fr-FR", {
                    day: "numeric",
                    month: "short",
                    year: "numeric",
                  })}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* ── Usage Guide ───────────────────── */}
      <Separator />
      <div className="pat-usage-guide">
        <h2 className="text-sm font-semibold mb-2">💡 Utilisation</h2>
        <p className="text-xs text-muted-foreground mb-3">
          Utilisez votre PAT comme mot de passe lors d&apos;un{" "}
          <code className="font-mono bg-muted px-1 rounded">git push</code> :
        </p>
        <pre className="pat-code-block">
          <code>
            {`git remote add shinobi http://localhost:3000/${user.handle}/mon-repo.git
git push shinobi main
# Username: ${user.handle}
# Password: shb_xxxxxxxxxxxxxxxxxxxxxxxx...`}
          </code>
        </pre>
      </div>
    </div>
  );
}
