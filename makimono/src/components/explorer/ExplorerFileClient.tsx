"use client";

// ═══════════════════════════════════════════════════════════════
// ExplorerFileClient — Wrapper client pour CodeViewer + Tensai
// Gère l'état d'ouverture du panneau Tensai IA (Phase 14).
// Nécessaire car la page explorer est un Server Component.
// ═══════════════════════════════════════════════════════════════

import React, { useState } from "react";
import CodeViewer from "@/components/explorer/CodeViewer";
import TensaiPanel from "@/components/explorer/TensaiPanel";
import { decodeBase64 } from "@/lib/explorer-api";

interface ExplorerFileClientProps {
  path: string;
  contentB64: string;
  language: string | null;
  isText: boolean;
  size: number;
}

export default function ExplorerFileClient({
  path,
  contentB64,
  language,
  isText,
  size,
}: ExplorerFileClientProps) {
  const [isTensaiOpen, setIsTensaiOpen] = useState(false);

  const fileContent = isText ? decodeBase64(contentB64) : "";

  return (
    <>
      <CodeViewer
        path={path}
        contentB64={contentB64}
        language={language}
        isText={isText}
        size={size}
        onTensaiClick={() => setIsTensaiOpen(true)}
      />
      <TensaiPanel
        isOpen={isTensaiOpen}
        onClose={() => setIsTensaiOpen(false)}
        filePath={path}
        fileContent={fileContent}
        language={language}
      />
    </>
  );
}
