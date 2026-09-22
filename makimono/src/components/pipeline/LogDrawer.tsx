"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Log Drawer (Phase 40-C)
// Panneau latéral pour afficher les logs d'un stage ⚡
//
// Vegapunk Tweaks intégrés :
// 🔬 #2 — Logs déjà tronqués par le backend (HEAD 100 + TAIL 200, 64KB max)
// 🔬 #3 — Smart Auto-Scroll : bloque le scroll si l'utilisateur
//          remonte de ≥50px, avec bouton flottant "↓ Suivre les logs"
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useRef, useState } from "react";
import type { PipelineStage, PipelineStageStatus } from "@/lib/pipeline-api";

// ── Props ────────────────────────────────────────────────────

interface LogDrawerProps {
  /** Stage dont on affiche les logs. */
  stage: PipelineStage | null;
  /** Contrôle l'ouverture du drawer. */
  open: boolean;
  /** Callback de fermeture. */
  onClose: () => void;
}

// ── Status Config ────────────────────────────────────────────

const STAGE_LABELS: Record<PipelineStageStatus, string> = {
  pending: "En attente",
  running: "En cours",
  success: "Succès",
  failure: "Échoué",
  error: "Erreur",
  skipped: "Ignoré",
};

// ── Component ────────────────────────────────────────────────

export function LogDrawer({ stage, open, onClose }: LogDrawerProps) {
  const logRef = useRef<HTMLPreElement>(null);
  const [isFollowing, setIsFollowing] = useState(true);

  // ── Escape key ──────────────────────────────────────────────
  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  // ── Body scroll lock ────────────────────────────────────────
  useEffect(() => {
    if (open) {
      document.body.style.overflow = "hidden";
      return () => {
        document.body.style.overflow = "";
      };
    }
  }, [open]);

  // ── Vegapunk Tweak #3 : Smart Auto-Scroll ───────────────────
  // Si l'utilisateur est remonté de ≥50px, on bloque l'auto-scroll.
  const handleScroll = useCallback(() => {
    const el = logRef.current;
    if (!el) return;
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setIsFollowing(distFromBottom < 50);
  }, []);

  // Auto-scroll uniquement si l'utilisateur "suit" les logs
  useEffect(() => {
    if (isFollowing && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [stage?.logs, isFollowing]);

  // Reset following state quand on ouvre un nouveau stage
  useEffect(() => {
    if (open) setIsFollowing(true);
  }, [open, stage?.id]);

  const scrollToBottom = useCallback(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
      setIsFollowing(true);
    }
  }, []);

  if (!open || !stage) return null;

  const statusLabel = STAGE_LABELS[stage.status] ?? stage.status;
  const durationStr = formatDuration(stage.duration_ms);

  return (
    <>
      {/* Overlay — clic pour fermer */}
      <div className="pl-drawer-overlay" onClick={onClose} />

      {/* Drawer panel */}
      <div className="pl-drawer">
        {/* Header */}
        <div className="pl-drawer-header">
          <div className="pl-drawer-title-group">
            <h3 className="pl-drawer-title">📜 {stage.name}</h3>
            <div className="pl-drawer-meta">
              <span className="pl-stage-image">{stage.image}</span>
              <span>{statusLabel}</span>
              {durationStr !== "—" && <span>{durationStr}</span>}
              {stage.exit_code !== null && stage.exit_code !== 0 && (
                <span className="pl-exit-code">exit {stage.exit_code}</span>
              )}
            </div>
          </div>
          <button
            className="pl-drawer-close"
            onClick={onClose}
            title="Fermer (Escape)"
          >
            ✕
          </button>
        </div>

        {/* Log content */}
        {stage.logs ? (
          <pre
            ref={logRef}
            className="pl-log-pre"
            onScroll={handleScroll}
          >
            <code>{stage.logs}</code>
          </pre>
        ) : (
          <div className="pl-log-empty">
            Aucun log disponible pour ce stage.
          </div>
        )}

        {/* Vegapunk Tweak #3 : Bouton flottant "Suivre" */}
        {stage.logs && !isFollowing && (
          <button className="pl-follow-btn" onClick={scrollToBottom}>
            ↓ Suivre les logs
          </button>
        )}
      </div>
    </>
  );
}

// ── Helpers ──────────────────────────────────────────────────

function formatDuration(ms: number | null): string {
  if (ms === null) return "—";
  const s = Math.floor(ms / 1000);
  if (s < 1) return "< 1s";
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  if (m < 60) return `${m}m ${rs}s`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return `${h}h ${rm}m`;
}
