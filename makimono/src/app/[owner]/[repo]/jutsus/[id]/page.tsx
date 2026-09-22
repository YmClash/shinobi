"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Jutsu Detail Page (Phase 40-C)
// Route: /[owner]/[repo]/jutsus/[id]
// ═══════════════════════════════════════════════════════════════

import { useParams } from "next/navigation";
import { PipelineDetail } from "@/components/pipeline/PipelineDetail";

export default function JutsuDetailPage() {
  const params = useParams<{ owner: string; repo: string; id: string }>();
  if (!params.owner || !params.repo || !params.id) return null;

  return (
    <PipelineDetail owner={params.owner} repo={params.repo} id={params.id} />
  );
}
