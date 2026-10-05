"use client";

// ═══════════════════════════════════════════════════════════════
// /auth/github/callback — Page de retour OAuth GitHub (Phase 20)
// Reçoit code+state de GitHub, les échange via le backend,
// stocke le JWT et redirige vers la Forge.
// ═══════════════════════════════════════════════════════════════

import { useEffect, useState, useRef } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense } from "react";
import {
  exchangeGitHubCode,
  setToken,
  setStoredUser,
} from "@/lib/auth";

function GitHubCallbackInner() {
  const router = useRouter();
  const params = useSearchParams();
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<"loading" | "success" | "error">("loading");
  // Guard against double-invocation (React Strict Mode)
  const exchangedRef = useRef(false);

  useEffect(() => {
    // Prevent double exchange (StrictMode re-mount)
    if (exchangedRef.current) return;

    const code = params.get("code");
    const state = params.get("state");

    if (!code || !state) {
      setError("Paramètres OAuth manquants. Réessayez depuis la page de connexion.");
      setStatus("error");
      return;
    }

    exchangedRef.current = true;

    exchangeGitHubCode(code, state)
      .then((result) => {
        // Stocker le JWT et les infos utilisateur
        setToken(result.token);
        setStoredUser(result.actor);

        // Émettre l'événement auth pour synchroniser le sidebar
        window.dispatchEvent(new Event("auth-change"));

        setStatus("success");

        // Rediriger vers la Forge après un bref délai
        setTimeout(() => router.push("/"), 500);
      })
      .catch((err) => {
        const msg =
          err instanceof Error
            ? err.message
            : "Erreur lors de l'authentification GitHub";
        setError(msg);
        setStatus("error");
      });
  }, [params, router]);

  return (
    <div className="auth-page">
      <div className="auth-card animate-fade-in-up">
        <div className="auth-logo">
          <span className="auth-logo-icon">🥷</span>
          <div className="auth-logo-text">
            <span className="auth-logo-brand">SHINOBI</span>
            <span className="auth-logo-sub">Forge Sociale</span>
          </div>
        </div>

        {status === "error" && (
          <>
            <h1 className="auth-title">Erreur OAuth</h1>
            <div className="auth-error">
              <span>⚠️</span>
              <span>{error}</span>
            </div>
            <div className="auth-footer" style={{ marginTop: "1.5rem", display: "flex", flexDirection: "column", gap: "0.75rem", alignItems: "center" }}>
              <a href="/login" className="auth-link">
                ← Retour à la connexion
              </a>
              <button
                onClick={() => {
                  // Retry: get a new OAuth URL and redirect
                  window.location.href = "/login";
                }}
                className="auth-link"
                style={{ cursor: "pointer", background: "none", border: "none", font: "inherit" }}
              >
                🔄 Réessayer
              </button>
            </div>
          </>
        )}

        {status === "success" && (
          <>
            <h1 className="auth-title">Connexion réussie ✅</h1>
            <p className="auth-subtitle">
              Redirection vers la Forge en cours...
            </p>
            <div style={{ display: "flex", justifyContent: "center", padding: "1.5rem" }}>
              <span className="auth-spinner" />
            </div>
          </>
        )}

        {status === "loading" && (
          <>
            <h1 className="auth-title">Connexion via GitHub...</h1>
            <p className="auth-subtitle">
              Échange des clés d&apos;accès en cours. Veuillez patienter.
            </p>
            <div style={{ display: "flex", justifyContent: "center", padding: "2rem" }}>
              <span className="auth-spinner" />
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default function GitHubCallbackPage() {
  return (
    <Suspense
      fallback={
        <div className="auth-page">
          <div className="auth-card animate-fade-in-up">
            <div style={{ display: "flex", justifyContent: "center", padding: "2rem" }}>
              <span className="auth-spinner" />
            </div>
          </div>
        </div>
      }
    >
      <GitHubCallbackInner />
    </Suspense>
  );
}
