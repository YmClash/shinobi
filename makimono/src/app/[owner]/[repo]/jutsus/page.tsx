"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Jutsus List Page (Phase 40-C)
// Route: /[owner]/[repo]/jutsus
// ═══════════════════════════════════════════════════════════════

import { useParams } from "next/navigation";
import { PipelineList } from "@/components/pipeline/PipelineList";

export default function JutsusPage() {
  const params = useParams<{ owner: string; repo: string }>();
  if (!params.owner || !params.repo) return null;

  return <PipelineList owner={params.owner} repo={params.repo} />;
}
