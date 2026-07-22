"use client";

import React, { useState, useCallback, useEffect } from "react";
import { archiveRepository } from "@/lib/api";
import { getToken } from "@/lib/auth";
import { useRouter } from "next/navigation";

// ── Word lists for generating confirmation phrases ──────────────
const WORDS_A = [
  "KATANA", "SHADOW", "PHOENIX", "STORM", "BLADE",
  "FORGE", "RAVEN", "VIPER", "INFERNO", "TITAN",
  "ECLIPSE", "CIPHER", "APEX", "ZENITH", "VOID",
];
const WORDS_B = [
  "FORGE", "STRIKE", "DAWN", "PULSE", "CORE",
  "EDGE", "DRIFT", "FLUX", "NEXUS", "SPARK",
  "PRISM", "ORBIT", "WAVE", "BLAZE", "CREST",
];

function generateChallengeWord(): string {
  const a = WORDS_A[Math.floor(Math.random() * WORDS_A.length)];
  const b = WORDS_B[Math.floor(Math.random() * WORDS_B.length)];
  return `${a}-${b}`;
}

// ── Component ──────────────────────────────────────────────────

interface DeleteRepoModalProps {
  isOpen: boolean;
  onClose: () => void;
  owner: string;
  repo: string;
  displayName: string;
}

export default function DeleteRepoModal({
  isOpen,
  onClose,
  owner,
  repo,
  displayName,
}: DeleteRepoModalProps) {
  const router = useRouter();
  const [challengeWord, setChallengeWord] = useState("");
  const [inputValue, setInputValue] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setChallengeWord(generateChallengeWord());
      setInputValue("");
      setError(null);
    }
  }, [isOpen]);

  const isMatch = inputValue === challengeWord;

  const handleDelete = useCallback(async () => {
    if (!isMatch) return;
    setLoading(true);
    setError(null);

    try {
      const token = getToken();
      if (!token) {
        setError("Vous devez être connecté pour supprimer un dépôt.");
        setLoading(false);
        return;
      }

      await archiveRepository(owner, repo, challengeWord, challengeWord, token);
      onClose();
      router.push("/forge");
      router.refresh();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Erreur inconnue";
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [isMatch, owner, repo, challengeWord, onClose, router]);

  if (!isOpen) return null;

  return (
    <div className="del-overlay" onClick={onClose}>
      <div className="del-modal" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div className="del-header">
          <div className="del-header-icon">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
            </svg>
          </div>
          <h2>Supprimer le dépôt</h2>
          <button className="del-close" onClick={onClose} aria-label="Fermer">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Body */}
        <div className="del-body">
          <div className="del-warning">
            <p>
              Le dépôt <strong>{displayName}</strong> sera mis en corbeille pendant
              <strong> 1 heure</strong>. Après ce délai, il sera supprimé
              <strong> définitivement</strong> avec toutes ses données.
            </p>
            <p className="del-warning-sub">
              Vous pourrez le restaurer depuis la corbeille pendant ce délai.
            </p>
          </div>

          <div className="del-challenge">
            <p>Pour confirmer, tapez le mot suivant :</p>
            <div className="del-word">{challengeWord}</div>
            <input
              id="delete-confirmation-input"
              type="text"
              value={inputValue}
              onChange={(e) => setInputValue(e.target.value.toUpperCase())}
              placeholder="Tapez le mot de confirmation"
              autoComplete="off"
              autoFocus
              className={`del-input ${isMatch ? "match" : ""}`}
            />
          </div>

          {error && (
            <div className="del-error">
              <span>⚠️</span> {error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="del-footer">
          <button className="del-btn-cancel" onClick={onClose} disabled={loading}>
            Annuler
          </button>
          <button
            className="del-btn-delete"
            onClick={handleDelete}
            disabled={!isMatch || loading}
          >
            {loading ? (
              <span className="del-spinner" />
            ) : (
              <>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z" />
                </svg>
                Supprimer définitivement
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
