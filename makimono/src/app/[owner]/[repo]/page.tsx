// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo] — Redirection vers l'explorateur
// Redirige vers /[owner]/[repo]/tree/{default_branch}
// ═══════════════════════════════════════════════════════════════

import { redirect } from "next/navigation";
import { getRepository } from "@/lib/api";
import type { Metadata } from "next";

interface PageProps {
  params: Promise<{ owner: string; repo: string }>;
}

export async function generateMetadata({
  params,
}: PageProps): Promise<Metadata> {
  const { owner, repo } = await params;
  return {
    title: `${owner}/${repo} — SHINOBI`,
  };
}

export default async function RepoHomePage({ params }: PageProps) {
  const { owner, repo } = await params;

  // Récupérer la branche par défaut depuis l'API
  let defaultBranch = "main";
  try {
    const repoMeta = await getRepository(owner, repo);
    defaultBranch = repoMeta.default_branch || "main";
  } catch {
    // Pas de méta → on tente "main" directement
  }

  redirect(`/${owner}/${repo}/tree/${defaultBranch}`);
}
