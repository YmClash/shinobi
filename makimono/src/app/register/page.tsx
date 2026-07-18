"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/hooks/use-auth";
import { getGitHubAuthUrl } from "@/lib/auth";

export default function RegisterPage() {
  const router = useRouter();
  const { register } = useAuth();
  const [handle, setHandle] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [githubLoading, setGithubLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    // Client-side validation
    if (password.length < 8) {
      setError("Le mot de passe doit contenir au moins 8 caractères");
      return;
    }
    if (password !== confirmPassword) {
      setError("Les mots de passe ne correspondent pas");
      return;
    }
    if (!/^[a-z0-9_-]+$/.test(handle)) {
      setError(
        "Le handle ne peut contenir que des lettres minuscules, chiffres, tirets et underscores"
      );
      return;
    }

    setSubmitting(true);
    try {
      await register({
        handle,
        display_name: displayName || handle,
        email,
        password,
      });
      router.push("/");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Erreur d'inscription");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="auth-page">
      <div className="auth-card animate-fade-in-up">
        {/* ── Logo ──────────────────────────── */}
        <div className="auth-logo">
          <span className="auth-logo-icon">🥷</span>
          <div className="auth-logo-text">
            <span className="auth-logo-brand">SHINOBI</span>
            <span className="auth-logo-sub">Forge Sociale</span>
          </div>
        </div>

        <h1 className="auth-title">Créer un compte</h1>
        <p className="auth-subtitle">
          Rejoignez la Forge. Forgez vos premiers jutsu.
        </p>

        {/* ── Error ─────────────────────────── */}
        {error && (
          <div className="auth-error">
            <span>⚠️</span>
            <span>{error}</span>
          </div>
        )}

        {/* ── GitHub OAuth Button ──────────────── */}
        <button
          type="button"
          onClick={async () => {
            setError(null);
            setGithubLoading(true);
            try {
              const url = await getGitHubAuthUrl();
              window.location.href = url;
            } catch (err: unknown) {
              setError(err instanceof Error ? err.message : "GitHub OAuth non disponible");
              setGithubLoading(false);
            }
          }}
          disabled={githubLoading}
          className="auth-github-btn"
        >
          {githubLoading ? (
            <span className="auth-spinner" />
          ) : (
            <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
            </svg>
          )}
          Continuer avec GitHub
        </button>

        {/* ── Divider ───────────────────────── */}
        <div className="auth-divider">
          <span>ou</span>
        </div>

        {/* ── Form ──────────────────────────── */}
        <form onSubmit={handleSubmit} className="auth-form">
          <div className="auth-field-row">
            <div className="auth-field">
              <label htmlFor="reg-handle" className="auth-label">
                Handle
              </label>
              <input
                id="reg-handle"
                type="text"
                required
                autoComplete="username"
                placeholder="ninja-dev"
                value={handle}
                onChange={(e) => setHandle(e.target.value.toLowerCase())}
                className="auth-input"
                disabled={submitting}
                maxLength={39}
              />
            </div>

            <div className="auth-field">
              <label htmlFor="reg-display" className="auth-label">
                Nom affiché
              </label>
              <input
                id="reg-display"
                type="text"
                autoComplete="name"
                placeholder="Ninja Dev"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                className="auth-input"
                disabled={submitting}
              />
            </div>
          </div>

          <div className="auth-field">
            <label htmlFor="reg-email" className="auth-label">
              Email
            </label>
            <input
              id="reg-email"
              type="email"
              required
              autoComplete="email"
              placeholder="ninja@shinobi.dev"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="auth-input"
              disabled={submitting}
            />
          </div>

          <div className="auth-field-row">
            <div className="auth-field">
              <label htmlFor="reg-password" className="auth-label">
                Mot de passe
              </label>
              <input
                id="reg-password"
                type="password"
                required
                autoComplete="new-password"
                placeholder="••••••••"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="auth-input"
                disabled={submitting}
                minLength={8}
              />
            </div>

            <div className="auth-field">
              <label htmlFor="reg-confirm" className="auth-label">
                Confirmer
              </label>
              <input
                id="reg-confirm"
                type="password"
                required
                autoComplete="new-password"
                placeholder="••••••••"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                className="auth-input"
                disabled={submitting}
                minLength={8}
              />
            </div>
          </div>

          <button
            type="submit"
            disabled={submitting}
            className="auth-submit"
          >
            {submitting ? (
              <>
                <span className="auth-spinner" />
                Inscription...
              </>
            ) : (
              "Créer mon compte"
            )}
          </button>
        </form>

        {/* ── Footer ────────────────────────── */}
        <div className="auth-footer">
          <span>Déjà un compte ?</span>
          <Link href="/login" className="auth-link">
            Se connecter
          </Link>
        </div>
      </div>
    </div>
  );
}
