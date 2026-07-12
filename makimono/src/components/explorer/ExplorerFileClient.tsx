"use client";

// ═══════════════════════════════════════════════════════════════
// ExplorerFileClient — Wrapper client pour CodeViewer + Sensei
// Gère l'état d'ouverture du panneau Sensei IA (Phase 15).
// Nécessaire car la page explorer est un Server Component.
// ═══════════════════════════════════════════════════════════════

import React, { useState } from "react";
import CodeViewer from "@/components/explorer/CodeViewer";
import SenseiPanel from "@/components/explorer/SenseiPanel";
import { decodeBase64 } from "@/lib/explorer-api";

interface ExplorerFileClientProps {
  path: string;
  contentB64: string;
  language: string | null;
  isText: boolean;
  size: number;
  /** Owner du dépôt (ex: "system") — Phase 15: Sensei context. */
  owner: string;
  /** Nom du dépôt (ex: "hello-world") — Phase 15: Sensei context. */
  repo: string;
}

export default function ExplorerFileClient({
  path,
  contentB64,
  language,
  isText,
  size,
  owner,
  repo,
}: ExplorerFileClientProps) {
  const [isSenseiOpen, setIsSenseiOpen] = useState(false);

  const fileContent = isText ? decodeBase64(contentB64) : "";

  return (
    <>
      <CodeViewer
        path={path}
        contentB64={contentB64}
        language={language}
        isText={isText}
        size={size}
        onTensaiClick={() => setIsSenseiOpen(true)}
      />
      <SenseiPanel
        isOpen={isSenseiOpen}
        onClose={() => setIsSenseiOpen(false)}
        filePath={path}
        fileContent={fileContent}
        language={language}
        owner={owner}
        repo={repo}
      />
    </>
  );
}
