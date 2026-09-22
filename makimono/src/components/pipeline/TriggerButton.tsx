"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Trigger Button (Phase 40-C)
// Bouton de déclenchement manuel d'un pipeline Jutsu ⚡
// ═══════════════════════════════════════════════════════════════

import { useState, useCallback } from "react";
import { triggerPipeline } from "@/lib/pipeline-api";

// ── Props ────────────────────────────────────────────────────

interface TriggerButtonProps {
  owner: string;
  repo: string;
  /** Callback après trigger réussi (pour refetch la liste). */
  onTriggered?: () => void;
}

// ── Component ────────────────────────────────────────────────

export function TriggerButton({ owner, repo, onTriggered }: TriggerButtonProps) {
  const [open, setOpen] = useState(false);
  const [commitId, setCommitId] = useState("");
  const [refName, setRefName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const handleSubmit = useCallback(async () => {
    if (!commitId.trim()) return;
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const result = await triggerPipeline(owner, repo, {
        commit_id: commitId.trim(),
        ref_name: refName.trim() || undefined,
      });
      setSuccess(`Pipeline ${result.pipeline_id.slice(0, 8)}… créé ⚡`);
      setCommitId("");
      setRefName("");
      onTriggered?.();
      setTimeout(() => {
        setOpen(false);
        setSuccess(null);
      }, 2000);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erreur inconnue");
    } finally {
      setLoading(false);
    }
  }, [owner, repo, commitId, refName, onTriggered]);

  const handleClose = useCallback(() => {
    setOpen(false);
    setError(null);
    setSuccess(null);
  }, []);

  return (
    <>
      <button className="pl-trigger-btn" onClick={() => setOpen(true)}>
        ⚡ Trigger
      </button>

      {open && (
        <div className="pl-trigger-modal-backdrop" onClick={handleClose}>
          <div className="pl-trigger-modal" onClick={(e) => e.stopPropagation()}>
            <div className="pl-trigger-modal-title">⚡ Déclencher un Jutsu</div>

            <div className="pl-form-group">
              <label className="pl-form-label">Commit SHA *</label>
              <input
                className="pl-form-input"
                value={commitId}
                onChange={(e) => setCommitId(e.target.value)}
                placeholder="abc1234def5678..."
                autoFocus
              />
              <div className="pl-form-hint">
                SHA complet ou abrégé du commit à builder.
              </div>
            </div>

            <div className="pl-form-group">
              <label className="pl-form-label">Branche (optionnel)</label>
              <input
                className="pl-form-input"
                value={refName}
                onChange={(e) => setRefName(e.target.value)}
                placeholder="main"
              />
              <div className="pl-form-hint">
                Nom de la branche source, pour référence.
              </div>
            </div>

            {error && <div className="pl-trigger-error">⚠️ {error}</div>}
            {success && <div className="pl-trigger-success">✅ {success}</div>}

            <div className="pl-trigger-actions">
              <button className="pl-btn-cancel" onClick={handleClose}>
                Annuler
              </button>
              <button
                className="pl-btn-submit"
                onClick={handleSubmit}
                disabled={loading || !commitId.trim()}
              >
                {loading ? "Envoi…" : "⚡ Lancer le Jutsu"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
