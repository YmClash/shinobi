import { notFound } from "next/navigation";
import { Metadata } from "next";
import Link from "next/link";
import { getActorProfile, listRepositories, type ActorProfile } from "@/lib/api";
import { getBaseUrl } from "@/lib/api";
import { RepoCard } from "@/components/forge/repo-card";
import { ProfileTimeline } from "@/components/profile/profile-timeline";

// ═══════════════════════════════════════════════════════════════
// Phase 37A — Le Visage Public (Actor Profile Page)
//
// React Server Component pour le SEO.
// Les données sont fetchées côté serveur avant le rendu HTML.
// Seuls les composants interactifs (tabs, Follow) sont Client Components.
// ═══════════════════════════════════════════════════════════════

interface ProfilePageProps {
  params: Promise<{ owner: string }>;
}

// ── Dynamic Metadata (SEO) ────────────────────────────────────

export async function generateMetadata(
  { params }: ProfilePageProps
): Promise<Metadata> {
  const { owner } = await params;
  try {
    const profile = await getActorProfile(owner);
    return {
      title: `${profile.actor.display_name} (@${profile.actor.handle}) — SHINOBI`,
      description: profile.actor.bio || `Profil de ${profile.actor.display_name} sur la Forge Sociale SHINOBI.`,
    };
  } catch {
    return {
      title: `${owner} — SHINOBI`,
      description: `Profil de ${owner} sur la Forge Sociale SHINOBI.`,
    };
  }
}

// ── Helpers ───────────────────────────────────────────────────

function timeAgo(dateStr: string | null): string {
  if (!dateStr) return "";
  const diff = Date.now() - new Date(dateStr).getTime();
  const days = Math.floor(diff / 86_400_000);
  if (days < 1) return "aujourd'hui";
  if (days < 30) return `il y a ${days}j`;
  const months = Math.floor(days / 30);
  if (months < 12) return `il y a ${months} mois`;
  const years = Math.floor(months / 12);
  return `il y a ${years} an${years > 1 ? "s" : ""}`;
}

function actorTypeBadge(type: string) {
  switch (type) {
    case "human":
      return { emoji: "👤", label: "Humain", color: "border-emerald-500/30 text-emerald-500 bg-emerald-500/5" };
    case "ai_agent":
      return { emoji: "🤖", label: "Agent IA", color: "border-violet-500/30 text-violet-500 bg-violet-500/5" };
    case "system":
      return { emoji: "⚙️", label: "Système", color: "border-amber-500/30 text-amber-500 bg-amber-500/5" };
    default:
      return { emoji: "❓", label: type, color: "border-muted text-muted-foreground bg-muted/5" };
  }
}

// ── Main Component (RSC) ──────────────────────────────────────

export default async function ProfilePage({ params }: ProfilePageProps) {
  const { owner } = await params;

  let profile: ActorProfile;
  let repos: { repositories: Array<{ id: string; name: string; display_name: string; description: string | null; visibility: string; default_branch: string; created_at: string; mirror_source_url?: string | null; mirror_synced_at?: string | null; owner_id: string }>; count: number };

  try {
    // Fetch en parallèle côté serveur (SSR → SEO optimal)
    const [profileRes, reposRes] = await Promise.all([
      getActorProfile(owner),
      listRepositories(owner),
    ]);
    profile = profileRes;
    repos = reposRes;
  } catch {
    notFound();
  }

  const badge = actorTypeBadge(profile.actor.actor_type);
  const publicRepos = repos.repositories.filter(r => r.visibility === "public");
  const initial = (profile.actor.display_name || profile.actor.handle || "?")[0].toUpperCase();
  // Défensif : les champs Phase 37A peuvent être absents si le backend n'est pas à jour
  const activities = profile.recent_activities ?? [];
  const fediverseAddr = profile.fediverse_address ?? `@${profile.actor.handle}`;
  const followerCount = profile.stats?.follower_count ?? 0;

  return (
    <div className="max-w-4xl mx-auto space-y-8 py-2">
      {/* ── Profile Header ───────────────────────────── */}
      <div className="relative rounded-2xl border border-border/50 bg-card overflow-hidden">
        {/* Gradient Banner */}
        <div className="h-28 bg-gradient-to-r from-primary/20 via-primary/10 to-transparent" />

        <div className="px-6 pb-6 -mt-12">
          <div className="flex items-end gap-4 mb-4">
            {/* Avatar */}
            {profile.actor.avatar_url ? (
              <img
                src={profile.actor.avatar_url}
                alt={profile.actor.display_name}
                className="w-20 h-20 rounded-2xl border-4 border-card shadow-lg object-cover"
              />
            ) : (
              <div className="w-20 h-20 rounded-2xl border-4 border-card shadow-lg bg-primary/10 flex items-center justify-center">
                <span className="text-2xl font-bold text-primary">{initial}</span>
              </div>
            )}

            <div className="flex-1 min-w-0 pb-1">
              <div className="flex items-center gap-2 flex-wrap">
                <h1 className="text-xl font-bold text-foreground truncate">
                  {profile.actor.display_name}
                </h1>
                <span className={`inline-flex items-center gap-1 text-[10px] px-2 py-0.5 rounded-full border ${badge.color}`}>
                  {badge.emoji} {badge.label}
                </span>
              </div>
              <p className="text-sm text-muted-foreground font-mono">
                @{profile.actor.handle}
              </p>
            </div>

            {/* Follow Button (disabled V1) */}
            <div className="shrink-0">
              <button
                disabled
                title="En cours de forge — bientôt disponible"
                className="
                  inline-flex items-center gap-1.5 px-4 py-2 text-xs font-medium rounded-lg
                  border border-border/50 bg-muted/50 text-muted-foreground
                  cursor-not-allowed opacity-60
                  transition-all
                "
              >
                <span>✦</span>
                <span>Follow</span>
              </button>
            </div>
          </div>

          {/* Bio */}
          {profile.actor.bio && (
            <p className="text-sm text-foreground/80 mb-3 max-w-2xl">
              {profile.actor.bio}
            </p>
          )}

          {/* Meta row */}
          <div className="flex items-center gap-4 text-xs text-muted-foreground flex-wrap">
            {/* Fediverse Address */}
            <span className="inline-flex items-center gap-1 font-mono text-primary/70 bg-primary/5 px-2 py-0.5 rounded-md">
              🌐 {fediverseAddr}
            </span>

            {profile.actor.created_at && (
              <span className="inline-flex items-center gap-1">
                📅 Membre depuis {timeAgo(profile.actor.created_at)}
              </span>
            )}

            {/* Parent (for bots) */}
            {profile.parent && (
              <Link
                href={`/${profile.parent.handle}`}
                className="inline-flex items-center gap-1 hover:text-primary transition-colors"
              >
                🔗 Créé par <span className="font-medium">{profile.parent.display_name}</span>
              </Link>
            )}
          </div>
        </div>
      </div>

      {/* ── Stats Bar ────────────────────────────────── */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        <StatCard
          icon="📦"
          label="Dépôts publics"
          value={profile.stats.public_repos}
        />
        <StatCard
          icon="🌐"
          label="Followers fédérés"
          value={followerCount}
        />
        <StatCard
          icon="🤖"
          label="Agents IA"
          value={profile.stats.bots_count}
        />
        <StatCard
          icon="⚡"
          label="Activités récentes"
          value={activities.length}
        />
      </div>

      {/* ── Repositories Grid ────────────────────────── */}
      <section>
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold text-foreground flex items-center gap-2">
            <span className="text-primary">📦</span>
            Dépôts publics
            <span className="text-xs text-muted-foreground font-normal ml-1">
              ({publicRepos.length})
            </span>
          </h2>
        </div>

        {publicRepos.length > 0 ? (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {publicRepos.map((repo, i) => (
              <RepoCard
                key={repo.id}
                repo={repo}
                ownerHandle={profile.actor.handle}
                className={`animate-fade-in-up stagger-${Math.min(i + 1, 5)}`}
              />
            ))}
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground rounded-xl border border-border/30 bg-card/50">
            <span className="text-4xl mb-2 opacity-30">🏗️</span>
            <p className="text-xs">Aucun dépôt public</p>
          </div>
        )}
      </section>

      {/* ── Activity Timeline ────────────────────────── */}
      {activities.length > 0 && (
        <section>
          <h2 className="text-sm font-semibold text-foreground flex items-center gap-2 mb-4">
            <span className="text-primary">⚡</span>
            Activité récente
          </h2>
          <ProfileTimeline activities={activities} />
        </section>
      )}
    </div>
  );
}

// ── Stat Card Sub-component ──────────────────────────────────

function StatCard({ icon, label, value }: { icon: string; label: string; value: number }) {
  return (
    <div className="rounded-xl border border-border/30 bg-card/50 p-3 text-center transition-all hover:border-primary/30 hover:bg-primary/5">
      <div className="text-lg mb-0.5">{icon}</div>
      <div className="text-xl font-bold text-foreground tabular-nums">{value}</div>
      <div className="text-[10px] text-muted-foreground">{label}</div>
    </div>
  );
}
