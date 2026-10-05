"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { GitFork } from "lucide-react";
import { forkRepository, ApiError } from "@/lib/api";
import { getToken, getStoredUser } from "@/lib/auth";

// ═══════════════════════════════════════════════════════════════
// ForkButton — Phase 37B — Fork Local (Le Dédoublement)
//
// Client Component intégré dans le repo header (ex-repo-actions).
// Utilise les classes `ex-btn` pour s'harmoniser avec Cloner/Supprimer.
// ═══════════════════════════════════════════════════════════════

interface ForkButtonProps {
  /** Handle du propriétaire du repo source. */
  owner: string;
  /** Nom (slug) du repo source. */
  repo: string;
  /** UUID du propriétaire du repo (pour détecter si c'est le nôtre). */
  repoOwnerId: string;
  /** Nombre de forks existants (affiché à côté de l'icône). */
  forkCount?: number;
}

export function ForkButton({
  owner,
  repo,
  repoOwnerId,
  forkCount = 0,
}: ForkButtonProps) {
  const router = useRouter();
  const [status, setStatus] = useState<"idle" | "loading" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string>("");

  const token = getToken();
  const storedUser = getStoredUser();
  const currentUserHandle = storedUser?.handle ?? null;

  // Ne pas afficher si anonyme ou si c'est le propriétaire
  if (!currentUserHandle || !token) return null;
  if (storedUser?.id === repoOwnerId) return null;

  const handleFork = async () => {
    setStatus("loading");
    setErrorMsg("");

    try {
      const forked = await forkRepository(owner, repo, token);
      // Redirect vers le repo forké
      router.push(`/${currentUserHandle}/${forked.name}`);
    } catch (err) {
      setStatus("error");
      if (err instanceof ApiError) {
        if (err.status === 409) {
          setErrorMsg("Vous possédez déjà un fork ou un repo de ce nom.");
        } else {
          setErrorMsg(err.message || "Erreur lors du fork.");
        }
      } else {
        setErrorMsg("Erreur inattendue.");
      }
      // Auto-dismiss error after 4s
      setTimeout(() => setStatus("idle"), 4000);
    }
  };

  return (
    <div className="clone-dropdown-wrapper">
      <button
        id="fork-button"
        onClick={handleFork}
        disabled={status === "loading"}
        className={`ex-btn ${
          status === "error"
            ? "ex-btn-destructive"
            : "ex-btn-ghost"
        }`}
        title={status === "error" ? errorMsg : "Forker ce dépôt"}
      >
        {status === "loading" ? (
          <>
            <span className="inline-block w-3.5 h-3.5 border-2 border-current border-t-transparent rounded-full animate-spin" />
            <span>Fork…</span>
          </>
        ) : (
          <>
            <GitFork size={14} />
            <span>Fork</span>
            {forkCount > 0 && (
              <span className="ex-fork-count">{forkCount}</span>
            )}
          </>
        )}
      </button>

      {/* Error tooltip */}
      {status === "error" && errorMsg && (
        <div className="clone-dropdown animate-fade-in-up" style={{ minWidth: "220px" }}>
          <div className="clone-dropdown-header">
            <span className="clone-dropdown-title" style={{ color: "hsl(0, 70%, 60%)" }}>
              ⚠️ {errorMsg}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
