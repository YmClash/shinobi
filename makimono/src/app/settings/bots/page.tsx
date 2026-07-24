"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/hooks/use-auth";
import {
  createServiceAccount,
  listServiceAccounts,
  deleteServiceAccount,
  type ServiceAccount,
} from "@/lib/auth";
import { Separator } from "@/components/ui/separator";

// ═══════════════════════════════════════════════════════════════
// Phase 25 — Service Accounts (L'Acte de Naissance)
// Gestion des bots IA — Créer, lister, supprimer
// ═══════════════════════════════════════════════════════════════

export default function BotsPage() {
  const router = useRouter();
  const { user, loading: authLoading } = useAuth();
  const [bots, setBots] = useState<ServiceAccount[]>([]);
  const [loading, setLoading] = useState(true);
  const [handle, setHandle] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [newToken, setNewToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);

  // Redirect if not authenticated
  useEffect(() => {
    if (!authLoading && !user) {
      router.push("/login");
    }
  }, [authLoading, user, router]);

  // Load bots
  const loadBots = useCallback(async () => {
    try {
      const res = await listServiceAccounts();
      setBots(res.service_accounts);
    } catch {
      setError("Impossible de charger vos Service Accounts");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (user) loadBots();
  }, [user, loadBots]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setNewToken(null);
    setCreating(true);
    try {
      const res = await createServiceAccount({
        handle: handle.toLowerCase().trim(),
        display_name: displayName.trim() || handle.trim(),
      });
      setNewToken(res.token);
      setHandle("");
      setDisplayName("");
      await loadBots();
    } catch (err: unknown) {
      setError(
        err instanceof Error ? err.message : "Erreur de création du bot"
      );
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (botId: string) => {
    setDeleting(botId);
    setError(null);
    try {
      await deleteServiceAccount(botId);
      setDeleteConfirm(null);
      await loadBots();
    } catch (err: unknown) {
      setError(
        err instanceof Error ? err.message : "Erreur de suppression du bot"
      );
    } finally {
      setDeleting(null);
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
          🤖 Service Accounts
        </h1>
        <p className="text-sm text-muted-foreground mt-1">
          Les Service Accounts sont des agents IA qui héritent de vos permissions.
          Ils peuvent publier du code via{" "}
          <code className="text-xs font-mono bg-muted px-1.5 py-0.5 rounded">
            git push
          </code>{" "}
          en utilisant leur propre PAT.
        </p>
      </div>

      <Separator />

      {/* ── Create Bot ──────────────────── */}
      <div className="bot-create-section">
        <h2 className="text-sm font-semibold mb-3">Créer un nouveau bot</h2>

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
                Bot créé avec succès — PAT initial généré
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

        <form onSubmit={handleCreate} className="bot-create-form">
          <div className="bot-form-row">
            <input
              type="text"
              placeholder="Handle (ex: my-oracle-bot)"
              value={handle}
              onChange={(e) => setHandle(e.target.value)}
              className="auth-input"
              disabled={creating}
              required
              pattern="[a-z0-9\-_]+"
              maxLength={39}
            />
            <input
              type="text"
              placeholder="Nom d'affichage (optionnel)"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              className="auth-input"
              disabled={creating}
              maxLength={64}
            />
          </div>
          <button
            type="submit"
            disabled={creating || !handle.trim()}
            className="auth-submit bot-create-btn"
          >
            {creating ? (
              <>
                <span className="auth-spinner" /> Création...
              </>
            ) : (
              "🤖 Créer le bot"
            )}
          </button>
        </form>
      </div>

      <Separator />

      {/* ── Bot List ────────────────────── */}
      <div>
        <h2 className="text-sm font-semibold mb-3">
          Mes bots ({bots.length})
        </h2>

        {loading ? (
          <div className="space-y-2">
            {[...Array(2)].map((_, i) => (
              <div
                key={i}
                className="h-16 rounded-lg bg-muted animate-pulse"
              />
            ))}
          </div>
        ) : bots.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground">
            <span className="text-3xl block mb-2 opacity-30">🤖</span>
            <p className="text-sm">Aucun Service Account</p>
            <p className="text-xs mt-1 opacity-60">
              Créez un bot pour automatiser vos workflows
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {bots.map((bot) => (
              <div key={bot.id} className="bot-item">
                <div className="bot-item-main">
                  <div className="bot-item-info">
                    <span className="bot-item-icon">🤖</span>
                    <div>
                      <Link href={`/profile/${bot.handle}`} className="bot-item-handle" style={{ textDecoration: "none" }}>
                        @{bot.handle}
                      </Link>
                      <span className="bot-item-name">
                        {bot.display_name}
                      </span>
                    </div>
                  </div>
                  <div className="bot-item-actions">
                    <span className="bot-item-date">
                      {new Date(bot.created_at).toLocaleDateString("fr-FR", {
                        day: "numeric",
                        month: "short",
                        year: "numeric",
                      })}
                    </span>
                    {deleteConfirm === bot.id ? (
                      <div className="bot-delete-confirm">
                        <button
                          onClick={() => handleDelete(bot.id)}
                          disabled={deleting === bot.id}
                          className="bot-delete-yes"
                        >
                          {deleting === bot.id ? "..." : "Oui, supprimer"}
                        </button>
                        <button
                          onClick={() => setDeleteConfirm(null)}
                          className="bot-delete-no"
                        >
                          Annuler
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => setDeleteConfirm(bot.id)}
                        className="bot-delete-btn"
                        title="Supprimer ce bot"
                      >
                        🗑️
                      </button>
                    )}
                  </div>
                </div>
                <div className="bot-item-badge">
                  <span className="bot-badge-type">AI Agent</span>
                  <span className="bot-badge-inherit">
                    ↑ Hérite de @{user.handle}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* ── Usage Guide ───────────────────── */}
      <Separator />
      <div className="pat-usage-guide">
        <h2 className="text-sm font-semibold mb-2">💡 Comment ça marche</h2>
        <div className="bot-guide-grid">
          <div className="bot-guide-card">
            <span className="bot-guide-step">1</span>
            <div>
              <p className="text-xs font-medium">Créer un bot</p>
              <p className="text-xs text-muted-foreground">
                Choisissez un handle unique et récupérez le PAT initial
              </p>
            </div>
          </div>
          <div className="bot-guide-card">
            <span className="bot-guide-step">2</span>
            <div>
              <p className="text-xs font-medium">Configurer l&apos;agent</p>
              <p className="text-xs text-muted-foreground">
                Utilisez le PAT dans votre CI/CD ou script d&apos;automatisation
              </p>
            </div>
          </div>
          <div className="bot-guide-card">
            <span className="bot-guide-step">3</span>
            <div>
              <p className="text-xs font-medium">Héritage RBAC</p>
              <p className="text-xs text-muted-foreground">
                Le bot accède automatiquement à tous vos dépôts privés
              </p>
            </div>
          </div>
        </div>
        <pre className="pat-code-block mt-4">
          <code>
            {`# Votre bot peut pusher sur vos repos
git remote add shinobi http://localhost:3000/${user.handle}/mon-repo.git
git push shinobi main
# Username: <bot-handle>
# Password: shb_xxxxxxxxxxxxxxxxxxxxxxxx...`}
          </code>
        </pre>
      </div>
    </div>
  );
}
