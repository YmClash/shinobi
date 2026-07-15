"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/hooks/use-auth";

export default function LoginPage() {
  const router = useRouter();
  const { login } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      await login(email, password);
      router.push("/");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Erreur de connexion");
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

        <h1 className="auth-title">Connexion</h1>
        <p className="auth-subtitle">
          Entrez dans la Forge. Vos jutsu vous attendent.
        </p>

        {/* ── Error ─────────────────────────── */}
        {error && (
          <div className="auth-error">
            <span>⚠️</span>
            <span>{error}</span>
          </div>
        )}

        {/* ── Form ──────────────────────────── */}
        <form onSubmit={handleSubmit} className="auth-form">
          <div className="auth-field">
            <label htmlFor="login-email" className="auth-label">
              Email
            </label>
            <input
              id="login-email"
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

          <div className="auth-field">
            <label htmlFor="login-password" className="auth-label">
              Mot de passe
            </label>
            <input
              id="login-password"
              type="password"
              required
              autoComplete="current-password"
              placeholder="••••••••"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="auth-input"
              disabled={submitting}
            />
          </div>

          <button
            type="submit"
            disabled={submitting}
            className="auth-submit"
          >
            {submitting ? (
              <>
                <span className="auth-spinner" />
                Connexion...
              </>
            ) : (
              "Se connecter"
            )}
          </button>
        </form>

        {/* ── Footer ────────────────────────── */}
        <div className="auth-footer">
          <span>Pas encore de compte ?</span>
          <Link href="/register" className="auth-link">
            Créer un compte
          </Link>
        </div>
      </div>
    </div>
  );
}
