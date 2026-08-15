"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useAuth } from "@/hooks/use-auth";
import { useHealth } from "@/hooks/use-api";
import { Badge } from "@/components/ui/badge";

export function Header() {
  const { data: health } = useHealth();
  const { user, loading: authLoading, logout } = useAuth();
  const pathname = usePathname();

  // Detect repo context from URL: /[owner]/[repo]/...
  const segments = pathname.split("/").filter(Boolean);
  const isRepoContext =
    segments.length >= 3 && segments[2] === "operations";
  const repoOwner = isRepoContext ? segments[0] : null;
  const repoName = isRepoContext ? segments[1] : null;

  return (
    <header className="h-12 border-b border-border bg-card/50 backdrop-blur-sm flex items-center justify-between px-4 flex-shrink-0">
      <div className="flex items-center gap-3">
        <h1 className="text-sm font-semibold tracking-wide">
          Dashboard de l&apos;Architecte
        </h1>
        {repoOwner && repoName && (
          <span className="repo-breadcrumb">
            <span>📦</span>
            <Link href={`/${repoOwner}`} className="hover:text-primary transition-colors">{repoOwner}</Link>
            <span className="text-muted-foreground/40">/</span>
            <span>{repoName}</span>
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {health?.version && (
          <Badge variant="outline" className="text-[10px] font-mono px-2 py-0.5">
            Taijutsu {health.version}
          </Badge>
        )}
        <Badge variant="secondary" className="text-[10px] font-mono px-2 py-0.5">
          Makimono v0.1.0
        </Badge>

        {/* ── Auth Zone (Phase 19A) ───────── */}
        {!authLoading && (
          <>
            {user ? (
              <div className="header-user-menu">
                <Link
                  href={`/${user.handle}`}
                  className="header-user-badge"
                  title="Voir mon profil"
                >
                  {user.avatar_url ? (
                    <img
                      src={user.avatar_url}
                      alt={user.handle}
                      className="header-user-avatar-img"
                    />
                  ) : (
                    <span className="header-user-avatar">
                      {user.handle.charAt(0).toUpperCase()}
                    </span>
                  )}
                  <span className="header-user-handle">{user.handle}</span>
                </Link>
                <button
                  onClick={logout}
                  className="header-logout-btn"
                  title="Déconnexion"
                >
                  🚪
                </button>
              </div>
            ) : (
              <Link href="/login" className="header-login-btn">
                Se connecter
              </Link>
            )}
          </>
        )}
      </div>
    </header>
  );
}
