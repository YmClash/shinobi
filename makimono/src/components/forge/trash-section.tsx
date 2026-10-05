"use client";

import React, { useState, useEffect, useCallback } from "react";
import { listTrashRepositories, restoreRepository, type TrashRepository } from "@/lib/api";
import { getToken } from "@/lib/auth";
import { useAuth } from "@/hooks/use-auth";

/**
 * Section corbeille affichée dans la page Forge.
 * Montre les repos soft-deleted avec un timer de rétention
 * et un bouton "Restaurer".
 */
export function TrashSection({ onRestored }: { onRestored?: () => void }) {
  const { user } = useAuth();
  const [trash, setTrash] = useState<TrashRepository[]>([]);
  const [loading, setLoading] = useState(false);
  const [restoring, setRestoring] = useState<string | null>(null);

  const fetchTrash = useCallback(async () => {
    const token = getToken();
    if (!user?.handle || !token) return;

    setLoading(true);
    try {
      const res = await listTrashRepositories(user.handle, token);
      setTrash(res.trash);
    } catch {
      // Silently ignore
    } finally {
      setLoading(false);
    }
  }, [user?.handle]);

  useEffect(() => {
    fetchTrash();
    const interval = setInterval(fetchTrash, 60_000);
    return () => clearInterval(interval);
  }, [fetchTrash]);

  const handleRestore = useCallback(
    async (repo: TrashRepository) => {
      const token = getToken();
      if (!user?.handle || !token) return;

      setRestoring(repo.id);
      try {
        await restoreRepository(user.handle, repo.name, token);
        setTrash((prev) => prev.filter((r) => r.id !== repo.id));
        onRestored?.();
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : "Erreur";
        alert(`Erreur de restauration: ${msg}`);
      } finally {
        setRestoring(null);
      }
    },
    [user?.handle, onRestored]
  );

  if (trash.length === 0 && !loading) return null;

  return (
    <div className="trash-section">
      <div className="trash-header">
        <span className="trash-header-icon">🗑️</span>
        <h3>Corbeille</h3>
        <span className="trash-count">{trash.length}</span>
      </div>

      {loading && trash.length === 0 && (
        <div className="trash-loading">Chargement...</div>
      )}

      <div className="trash-list">
        {trash.map((repo) => {
          const minutesLeft = Math.max(0, Math.floor(repo.seconds_until_purge / 60));
          const hoursLeft = Math.floor(minutesLeft / 60);
          const minsRemain = minutesLeft % 60;
          const timeStr = hoursLeft > 0 ? `${hoursLeft}h ${minsRemain}min` : `${minsRemain} min`;

          return (
            <div key={repo.id} className="trash-item">
              <div className="trash-item-info">
                <span className="trash-item-name">{repo.display_name}</span>
                <span className="trash-item-slug">{repo.name}</span>
                <span className="trash-item-timer">
                  ⏳ {timeStr} restant{minutesLeft > 1 ? "s" : ""}
                </span>
              </div>
              <button
                className="trash-restore-btn"
                onClick={() => handleRestore(repo)}
                disabled={restoring === repo.id}
              >
                {restoring === repo.id ? "⏳..." : "♻️ Restaurer"}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
