"use client";

import { useState, useCallback } from "react";
import { Skeleton } from "@/components/ui/skeleton";
import { RepoCard } from "@/components/forge/repo-card";
import { CreateRepoForm } from "@/components/forge/create-repo-form";
import { useRepositories, emitRepoCreated } from "@/hooks/use-api";

import { forgeRepository } from "./actions";

// ═══════════════════════════════════════════════════════════════
// La Forge — Repository listing & creation page
// ═══════════════════════════════════════════════════════════════

const OWNER_HANDLE = "system"; // SYSTEM_ACTOR handle (migration 006)

export default function ForgePage() {
  const [showForm, setShowForm] = useState(false);
  const { data, loading, error, refetch } = useRepositories(OWNER_HANDLE);

  const handleCreated = useCallback(() => {
    setShowForm(false);
    refetch();           // Mise à jour locale (forge page)
    emitRepoCreated();  // Notifie la sidebar et tout autre listener
  }, [refetch]);


  const repos = data?.repositories ?? [];

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      {/* ── Header ───────────────────────────────── */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-bold tracking-wide">
            🔨 La Forge
          </h1>
          <p className="text-xs text-muted-foreground mt-1">
            Forgez et gérez vos dépôts de versioning
          </p>
        </div>
        <div className="flex items-center gap-3">
          {!loading && (
            <span className="text-xs text-muted-foreground">
              {repos.length} dépôt{repos.length > 1 ? "s" : ""}
            </span>
          )}
          <button
            onClick={() => setShowForm(!showForm)}
            className={`
              inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg
              transition-all cursor-pointer
              ${
                showForm
                  ? "bg-muted text-muted-foreground hover:bg-muted/80"
                  : "bg-primary text-primary-foreground hover:bg-primary/90 hover:shadow-lg hover:shadow-primary/20 hover:scale-105 active:scale-95"
              }
            `}
          >
            <span>{showForm ? "✕" : "✦"}</span>
            <span>{showForm ? "Annuler" : "Nouveau Dépôt"}</span>
          </button>
        </div>
      </div>

      {/* ── Create Form (slide-down) ─────────────── */}
      {showForm && (
        <div className="animate-fade-in-up rounded-xl border border-border/50 bg-card p-5">
          <h2 className="text-sm font-semibold mb-4 flex items-center gap-2">
            <span>🔨</span>
            <span>Forger un Nouveau Dépôt</span>
          </h2>
          <CreateRepoForm onCreated={handleCreated} forgeAction={forgeRepository} />
        </div>
      )}

      {/* ── Error ────────────────────────────────── */}
      {error && (
        <div className="text-sm text-destructive bg-destructive/10 rounded-lg p-4 border border-destructive/20">
          Erreur de connexion : {error}
        </div>
      )}

      {/* ── Loading ──────────────────────────────── */}
      {loading && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {[...Array(4)].map((_, i) => (
            <Skeleton key={i} className="h-32 w-full rounded-xl" />
          ))}
        </div>
      )}

      {/* ── Repo Grid ────────────────────────────── */}
      {!loading && repos.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {repos.map((repo, i) => (
            <RepoCard
              key={repo.id}
              repo={repo}
              ownerHandle={OWNER_HANDLE}
              className={`animate-fade-in-up stagger-${Math.min(i + 1, 5)}`}
            />
          ))}
        </div>
      )}

      {/* ── Empty State ──────────────────────────── */}
      {!loading && !error && repos.length === 0 && (
        <div className="flex flex-col items-center justify-center py-20 text-muted-foreground">
          <span className="text-6xl mb-4 opacity-30">🏗️</span>
          <p className="text-sm font-medium">Aucun dépôt forgé</p>
          <p className="text-xs mt-2 opacity-60">
            Cliquez sur &quot;Nouveau Dépôt&quot; pour créer votre premier dépôt
          </p>
        </div>
      )}
    </div>
  );
}
