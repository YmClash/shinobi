"use client";

import { useCallback, useEffect, useState } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import { getActorProfile, type ActorProfile } from "@/lib/auth";
import { listRepositories, type Repository } from "@/lib/api";
import { Separator } from "@/components/ui/separator";

// ═══════════════════════════════════════════════════════════════
// Phase 25B — Profil Public Acteur
// Page publique /@handle — Affiche le profil d'un humain ou bot
// ═══════════════════════════════════════════════════════════════

export default function ProfilePage() {
  const params = useParams<{ handle: string }>();
  const handle = params.handle;

  const [profile, setProfile] = useState<ActorProfile | null>(null);
  const [repos, setRepos] = useState<Repository[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadProfile = useCallback(async () => {
    if (!handle) return;
    setLoading(true);
    setError(null);
    try {
      const [profileData, reposData] = await Promise.allSettled([
        getActorProfile(handle),
        listRepositories(handle),
      ]);

      if (profileData.status === "fulfilled") {
        setProfile(profileData.value);
      } else {
        setError("Profil introuvable");
        return;
      }

      if (reposData.status === "fulfilled") {
        setRepos(reposData.value.repositories);
      }
    } catch {
      setError("Erreur de chargement du profil");
    } finally {
      setLoading(false);
    }
  }, [handle]);

  useEffect(() => {
    loadProfile();
  }, [loadProfile]);

  if (loading) {
    return (
      <div className="profile-page">
        <div className="profile-loading">
          <div className="auth-spinner" />
          <p className="text-xs text-muted-foreground mt-3">Chargement du profil...</p>
        </div>
      </div>
    );
  }

  if (error || !profile) {
    return (
      <div className="profile-page">
        <div className="profile-not-found">
          <span className="text-5xl mb-3 block opacity-30">👤</span>
          <h1 className="text-lg font-bold">Profil introuvable</h1>
          <p className="text-sm text-muted-foreground mt-1">
            L&apos;acteur <code className="font-mono bg-muted px-1 rounded">@{handle}</code> n&apos;existe pas.
          </p>
          <Link href="/forge" className="profile-back-link mt-4">
            ← Retour à la Forge
          </Link>
        </div>
      </div>
    );
  }

  const { actor, stats, parent } = profile;
  const isBot = actor.actor_type === "ai_agent";
  const isHuman = actor.actor_type === "human";
  const memberSince = new Date(actor.created_at).toLocaleDateString("fr-FR", {
    month: "long",
    year: "numeric",
  });

  return (
    <div className="profile-page">
      {/* ── Profile Header Card ────────────── */}
      <div className="profile-header-card">
        <div className="profile-avatar-section">
          {actor.avatar_url ? (
            <img
              src={actor.avatar_url}
              alt={actor.handle}
              className="profile-avatar-img"
            />
          ) : (
            <div className="profile-avatar-fallback">
              {isBot ? "🤖" : actor.handle.charAt(0).toUpperCase()}
            </div>
          )}
          <span className={`profile-type-badge ${isBot ? "profile-badge-bot" : isHuman ? "profile-badge-human" : "profile-badge-system"}`}>
            {isBot ? "🤖 AI Agent" : isHuman ? "👤 Human" : "⚙️ System"}
          </span>
        </div>

        <div className="profile-info-section">
          <h1 className="profile-display-name">{actor.display_name}</h1>
          <p className="profile-handle">@{actor.handle}</p>
          {actor.bio && (
            <p className="profile-bio">{actor.bio}</p>
          )}

          {/* Parent info for bots */}
          {isBot && parent && (
            <div className="profile-parent-card">
              <span className="text-xs text-muted-foreground">Créé par</span>
              <Link href={`/profile/${parent.handle}`} className="profile-parent-link">
                {parent.avatar_url ? (
                  <img
                    src={parent.avatar_url}
                    alt={parent.handle}
                    className="profile-parent-avatar"
                  />
                ) : (
                  <span className="profile-parent-avatar-fb">
                    {parent.handle.charAt(0).toUpperCase()}
                  </span>
                )}
                <span className="profile-parent-name">
                  {parent.display_name}
                  <span className="profile-parent-handle">@{parent.handle}</span>
                </span>
              </Link>
            </div>
          )}
        </div>
      </div>

      {/* ── Stats Bar ──────────────────────── */}
      <div className="profile-stats">
        <div className="profile-stat">
          <span className="profile-stat-value">{stats.public_repos}</span>
          <span className="profile-stat-label">Dépôts publics</span>
        </div>
        {isHuman && (
          <div className="profile-stat">
            <span className="profile-stat-value">{stats.bots_count}</span>
            <span className="profile-stat-label">Bots IA</span>
          </div>
        )}
        <div className="profile-stat">
          <span className="profile-stat-value">{memberSince}</span>
          <span className="profile-stat-label">Membre depuis</span>
        </div>
        {isBot && (
          <div className="profile-stat">
            <span className="profile-stat-value">↑ RBAC</span>
            <span className="profile-stat-label">Héritage actif</span>
          </div>
        )}
      </div>

      <Separator />

      {/* ── Repos Section ──────────────────── */}
      <div className="profile-repos-section">
        <h2 className="text-sm font-semibold mb-3">
          📦 Dépôts {isBot ? "(hérités du parent)" : ""}
          <span className="profile-repos-count">{repos.length}</span>
        </h2>

        {repos.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground">
            <span className="text-3xl block mb-2 opacity-30">📦</span>
            <p className="text-sm">Aucun dépôt public</p>
          </div>
        ) : (
          <div className="profile-repos-grid">
            {repos.map((repo) => (
              <Link
                key={repo.id}
                href={`/${handle}/${repo.name}`}
                className="profile-repo-card"
              >
                <div className="profile-repo-header">
                  <span className="profile-repo-name">{repo.display_name}</span>
                  <span className={`profile-repo-vis ${repo.visibility === "private" ? "profile-vis-private" : ""}`}>
                    {repo.visibility === "private" ? "🔒" : "🌐"} {repo.visibility}
                  </span>
                </div>
                {repo.description && (
                  <p className="profile-repo-desc">{repo.description}</p>
                )}
                <div className="profile-repo-meta">
                  <span className="text-xs text-muted-foreground">
                    {new Date(repo.created_at).toLocaleDateString("fr-FR", {
                      day: "numeric",
                      month: "short",
                    })}
                  </span>
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
