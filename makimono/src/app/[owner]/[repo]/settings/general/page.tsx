"use client";

// ═══════════════════════════════════════════════════════════════
// General Settings — Phase 42-pre-Fix (Privacy by Design 🔒)
// /[owner]/[repo]/settings/general
// Carte « Confidentialité IA » : opt-in d'envoi du code à un LLM externe
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useState } from "react";
import { useParams } from "next/navigation";
import { getBaseUrl, type Repository } from "@/lib/api";
import { getToken } from "@/lib/auth";
import { useAuth } from "@/hooks/use-auth";
import { updateRepoSettings } from "@/lib/repo-settings-api";

export default function GeneralSettingsPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const { user } = useAuth();

  const [repo, setRepo] = useState<Repository | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Repo metadata (cache désactivé : on veut la valeur réelle du toggle)
  useEffect(() => {
    if (!params.owner || !params.repo) return;
    const url = `${getBaseUrl()}/api/v1/repos/${encodeURIComponent(params.owner)}/${encodeURIComponent(params.repo)}`;
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    const token = getToken();
    if (token) headers["Authorization"] = `Bearer ${token}`;

    fetch(url, { headers, cache: "no-store" })
      .then((res) => (res.ok ? res.json() : null))
      .then((data: Repository | null) => setRepo(data))
      .catch(() => setRepo(null))
      .finally(() => setLoading(false));
  }, [params.owner, params.repo]);

  const isOwner = Boolean(user && repo && user.id === repo.owner_id);
  const enabled = Boolean(repo?.allow_external_llm);

  const toggle = useCallback(async () => {
    if (!repo || !isOwner || saving) return;
    const next = !enabled;
    setSaving(true);
    setError(null);
    // Optimistic update — rollback on failure
    setRepo({ ...repo, allow_external_llm: next });
    try {
      const res = await updateRepoSettings(params.owner, params.repo, {
        allow_external_llm: next,
      });
      setRepo((r) => (r ? { ...r, allow_external_llm: res.allow_external_llm } : r));
    } catch (e) {
      setRepo({ ...repo, allow_external_llm: enabled });
      setError(e instanceof Error ? e.message : "Update failed");
    } finally {
      setSaving(false);
    }
  }, [repo, isOwner, saving, enabled, params.owner, params.repo]);

  return (
    <div className="st-page">
      <div className="st-page-header">
        <h1 className="st-page-title">⚙️ General</h1>
      </div>

      {loading && <div className="st-skeleton" />}

      {!loading && !repo && (
        <div className="st-error">⚠️ Repository not found or access denied.</div>
      )}

      {repo && (
        <section className="st-card" aria-labelledby="st-privacy-title">
          <div className="st-card-head">
            <div>
              <h2 id="st-privacy-title" className="st-card-title">
                🔒 AI Privacy
              </h2>
              <p className="st-card-sub">
                Controls where Kage Bunshin (auto-healing) may send your code.
              </p>
            </div>
            <span
              className={`st-badge ${enabled ? "st-badge-external" : "st-badge-local"}`}
              id="st-privacy-badge"
            >
              {enabled ? "🌐 External LLM allowed" : "🔒 Local only"}
            </span>
          </div>

          <div className="st-row">
            <div className="st-row-text">
              <label htmlFor="st-allow-external-llm" className="st-row-label">
                Allow external LLM providers
              </label>
              <p className="st-row-desc">
                When enabled, the failing stage&apos;s source files and error logs
                are sent to a <strong>third-party provider</strong> (e.g. Gemini).
                When disabled, Kage Bunshin only uses the local model — or skips
                the heal if none is available. Your code never leaves your
                infrastructure.
              </p>
            </div>

            <button
              id="st-allow-external-llm"
              type="button"
              role="switch"
              aria-checked={enabled}
              disabled={!isOwner || saving}
              onClick={toggle}
              className={`st-switch ${enabled ? "st-switch-on" : ""}`}
              title={isOwner ? "Toggle external LLM access" : "Only the repository owner can change this"}
            >
              <span className="st-switch-thumb" />
            </button>
          </div>

          {!isOwner && (
            <p className="st-note">
              🛡️ Only the repository owner can change this setting.
            </p>
          )}
          {error && <p className="st-error">⚠️ {error}</p>}
        </section>
      )}
    </div>
  );
}
