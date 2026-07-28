"use client";

import { useCallback, useEffect, useState } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import { getActorProfile, type ActorProfile } from "@/lib/auth";
import { listRepositories, type Repository } from "@/lib/api";

// ═══════════════════════════════════════════════════════════════
// Phase 25B — Profil Public Acteur (Redesign Premium)
// Page publique /profile/[handle]
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
      <div className="pf">
        <div className="pf-loading">
          <div className="pf-pulse-ring" />
          <p>Chargement du profil...</p>
        </div>
      </div>
    );
  }

  if (error || !profile) {
    return (
      <div className="pf">
        <div className="pf-not-found">
          <div className="pf-404-icon">?</div>
          <h1>Acteur introuvable</h1>
          <p>
            Le handle <code>@{handle}</code> n&apos;existe pas dans le système SHINOBI.
          </p>
          <Link href="/forge" className="pf-back-btn">
            ← Retour à la Forge
          </Link>
        </div>
      </div>
    );
  }

  const { actor, stats, parent } = profile;
  const isSystem = profile.is_system === true;
  const isBot = actor.actor_type === "ai_agent";
  const isHuman = actor.actor_type === "human";
  const memberSince = actor.created_at
    ? new Date(actor.created_at).toLocaleDateString("fr-FR", {
        month: "long",
        year: "numeric",
      })
    : null;

  // ── Phase 27-pre : Page dédiée Acteur Système ──────────
  if (isSystem) {
    return (
      <div className="pf">
        <div className="pf-hero pf-hero-system">
          <div className="pf-hero-pattern" />
          <div className="pf-hero-glow" />

          <div className="pf-hero-content">
            <div className="pf-avatar-wrap">
              <div className="pf-avatar pf-avatar-gen pf-avatar-system">⚙️</div>
              <div className="pf-avatar-ring pf-ring-system" />
            </div>

            <div className="pf-identity">
              <div className="pf-name-row">
                <h1 className="pf-name">{actor.display_name}</h1>
                <span className="pf-badge pf-badge-sys">⚙️ SYSTEM</span>
              </div>
              <p className="pf-handle">@{actor.handle}</p>
              {actor.bio && <p className="pf-bio">{actor.bio}</p>}
            </div>
          </div>
        </div>

        {/* Stats minimales */}
        <div className="pf-stats">
          <div className="pf-stat">
            <span className="pf-stat-num">∞</span>
            <span className="pf-stat-lbl">Uptime</span>
          </div>
          <div className="pf-stat-divider" />
          <div className="pf-stat">
            <span className="pf-stat-num">🔒</span>
            <span className="pf-stat-lbl">Non-authentifiable</span>
          </div>
          <div className="pf-stat-divider" />
          <div className="pf-stat">
            <span className="pf-stat-num">🛡️</span>
            <span className="pf-stat-lbl">Ghost User</span>
          </div>
        </div>

        {/* Section responsabilités */}
        <div className="pf-section">
          <div className="pf-section-head">
            <h2 className="pf-section-title">🔧 Responsabilités</h2>
          </div>
          <div className="pf-caps">
            <div className="pf-cap">
              <span className="pf-cap-icon">🔄</span>
              <div>
                <span className="pf-cap-title">Migrations</span>
                <span className="pf-cap-desc">Rattachement automatique des données lors des mises à jour de schéma</span>
              </div>
            </div>
            <div className="pf-cap">
              <span className="pf-cap-icon">👻</span>
              <div>
                <span className="pf-cap-title">Ghost User</span>
                <span className="pf-cap-desc">Réattribution des données orphelines (comptes supprimés)</span>
              </div>
            </div>
            <div className="pf-cap">
              <span className="pf-cap-icon">🌐</span>
              <div>
                <span className="pf-cap-title">Actions fédérées</span>
                <span className="pf-cap-desc">Origine des requêtes ActivityPub entrantes (ForgeFed)</span>
              </div>
            </div>
            <div className="pf-cap">
              <span className="pf-cap-icon">🏗️</span>
              <div>
                <span className="pf-cap-title">Dépôt système</span>
                <span className="pf-cap-desc">Propriétaire du dépôt par défaut et des opérations MVP</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="pf">
      {/* ── Hero Banner ────────────────────── */}
      <div className={`pf-hero ${isBot ? "pf-hero-bot" : "pf-hero-human"}`}>
        <div className="pf-hero-pattern" />
        <div className="pf-hero-glow" />

        <div className="pf-hero-content">
          {/* Avatar */}
          <div className="pf-avatar-wrap">
            {actor.avatar_url ? (
              <img
                src={actor.avatar_url}
                alt={actor.handle}
                className="pf-avatar"
              />
            ) : (
              <div className="pf-avatar pf-avatar-gen">
                {isBot ? "🤖" : actor.handle.charAt(0).toUpperCase()}
              </div>
            )}
            <div className={`pf-avatar-ring ${isBot ? "pf-ring-bot" : "pf-ring-human"}`} />
          </div>

          {/* Identity */}
          <div className="pf-identity">
            <div className="pf-name-row">
              <h1 className="pf-name">{actor.display_name}</h1>
              <span className={`pf-badge ${isBot ? "pf-badge-bot" : isHuman ? "pf-badge-human" : "pf-badge-sys"}`}>
                {isBot ? "🤖 AI AGENT" : isHuman ? "👤 HUMAN" : "⚙️ SYSTEM"}
              </span>
            </div>
            <p className="pf-handle">@{actor.handle}</p>
            {actor.bio && <p className="pf-bio">{actor.bio}</p>}
          </div>
        </div>

        {/* Parent link for bots */}
        {isBot && parent && (
          <Link href={`/profile/${parent.handle}`} className="pf-parent-chip">
            <span className="pf-parent-label">Créé par</span>
            <div className="pf-parent-info">
              {parent.avatar_url ? (
                <img src={parent.avatar_url} alt={parent.handle} className="pf-parent-av" />
              ) : (
                <span className="pf-parent-av pf-parent-av-fb">
                  {parent.handle.charAt(0).toUpperCase()}
                </span>
              )}
              <span className="pf-parent-name">{parent.display_name}</span>
              <span className="pf-parent-handle">@{parent.handle}</span>
            </div>
            <span className="pf-parent-arrow">→</span>
          </Link>
        )}
      </div>

      {/* ── Stats Row ──────────────────────── */}
      <div className="pf-stats">
        <div className="pf-stat">
          <span className="pf-stat-num">{stats.public_repos}</span>
          <span className="pf-stat-lbl">Dépôts publics</span>
        </div>
        <div className="pf-stat-divider" />
        {isHuman && (
          <>
            <div className="pf-stat">
              <span className="pf-stat-num">{stats.bots_count}</span>
              <span className="pf-stat-lbl">Bots IA</span>
            </div>
            <div className="pf-stat-divider" />
          </>
        )}
        <div className="pf-stat">
          <span className="pf-stat-num">{memberSince}</span>
          <span className="pf-stat-lbl">Membre depuis</span>
        </div>
        {isBot && (
          <>
            <div className="pf-stat-divider" />
            <div className="pf-stat pf-stat-rbac">
              <span className="pf-stat-num">⚡</span>
              <span className="pf-stat-lbl">RBAC hérité</span>
            </div>
          </>
        )}
      </div>

      {/* ── Repos ──────────────────────────── */}
      <div className="pf-section">
        <div className="pf-section-head">
          <h2 className="pf-section-title">
            {isBot ? "🔗 Dépôts accessibles" : "📦 Dépôts"}
          </h2>
          <span className="pf-section-count">{repos.length}</span>
        </div>

        {repos.length === 0 ? (
          <div className="pf-empty">
            <div className="pf-empty-icon">{isBot ? "🔗" : "📦"}</div>
            <p className="pf-empty-title">Aucun dépôt public</p>
            <p className="pf-empty-sub">
              {isBot
                ? "Ce bot héritera des dépôts de son créateur."
                : "Créez votre premier dépôt depuis la Forge."}
            </p>
          </div>
        ) : (
          <div className="pf-repos">
            {repos.map((repo) => (
              <Link
                key={repo.id}
                href={`/${handle}/${repo.name}`}
                className="pf-repo"
              >
                <div className="pf-repo-top">
                  <span className="pf-repo-icon">
                    {repo.visibility === "private" ? "🔒" : "📂"}
                  </span>
                  <span className="pf-repo-name">{repo.display_name}</span>
                  <span className={`pf-repo-vis ${repo.visibility === "private" ? "pf-vis-priv" : ""}`}>
                    {repo.visibility}
                  </span>
                </div>
                {repo.description && (
                  <p className="pf-repo-desc">{repo.description}</p>
                )}
                <div className="pf-repo-foot">
                  <span>
                    {new Date(repo.created_at).toLocaleDateString("fr-FR", {
                      day: "numeric",
                      month: "short",
                      year: "numeric",
                    })}
                  </span>
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>

      {/* ── Capabilities (bots only) ───────── */}
      {isBot && (
        <div className="pf-section">
          <div className="pf-section-head">
            <h2 className="pf-section-title">⚙️ Capacités</h2>
          </div>
          <div className="pf-caps">
            <div className="pf-cap">
              <span className="pf-cap-icon">🔑</span>
              <div>
                <span className="pf-cap-title">Authentification PAT</span>
                <span className="pf-cap-desc">Token personnel pour git push/pull</span>
              </div>
            </div>
            <div className="pf-cap">
              <span className="pf-cap-icon">🔗</span>
              <div>
                <span className="pf-cap-title">Héritage RBAC</span>
                <span className="pf-cap-desc">Accès aux repos du créateur</span>
              </div>
            </div>
            <div className="pf-cap">
              <span className="pf-cap-icon">🤖</span>
              <div>
                <span className="pf-cap-title">Commits autonomes</span>
                <span className="pf-cap-desc">Opérations traçables dans l&apos;historique</span>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
