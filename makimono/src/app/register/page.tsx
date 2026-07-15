"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useAuth } from "@/hooks/use-auth";

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
