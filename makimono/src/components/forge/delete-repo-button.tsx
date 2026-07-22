"use client";

import React, { useState, useEffect } from "react";
import DeleteRepoModal from "./delete-repo-modal";
import { getToken } from "@/lib/auth";

interface DeleteRepoButtonProps {
  owner: string;
  repo: string;
  displayName: string;
  ownerId: string;
}

/**
 * Bouton "Supprimer" visible uniquement si l'utilisateur connecté
 * est le propriétaire du dépôt. Affiche la modale de confirmation.
 */
export default function DeleteRepoButton({
  owner,
  repo,
  displayName,
  ownerId,
}: DeleteRepoButtonProps) {
  const [isOwner, setIsOwner] = useState(false);
  const [showModal, setShowModal] = useState(false);

  // Check if the logged-in user is the owner
  useEffect(() => {
    try {
      const token = getToken();
      if (!token) return;

      const parts = token.split(".");
      if (parts.length !== 3) return;
      const payload = JSON.parse(atob(parts[1]));
      const actorId = payload.sub || payload.actor_id;
      if (actorId === ownerId) {
        setIsOwner(true);
      }
    } catch {
      // Invalid token — ignore
    }
  }, [ownerId]);

  if (!isOwner) return null;

  return (
    <>
      <button
        className="ex-btn ex-btn-danger"
        onClick={() => setShowModal(true)}
        title="Supprimer ce dépôt"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z" />
        </svg>
        <span>Supprimer</span>
      </button>

      <DeleteRepoModal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
        owner={owner}
        repo={repo}
        displayName={displayName}
      />
    </>
  );
}
