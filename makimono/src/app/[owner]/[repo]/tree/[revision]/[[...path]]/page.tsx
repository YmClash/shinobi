// ═══════════════════════════════════════════════════════════════
// SHINOBI — Code Explorer Page
// Route: /[owner]/[repo]/tree/[revision]/[[...path]]
//
// Server Component : fetch API + passe au Client Components.
// Un seul appel API determine si on affiche FileBrowser ou CodeViewer.
// ═══════════════════════════════════════════════════════════════

import React from "react";
import type { Metadata } from "next";
import Link from "next/link";
import { GitBranch, ExternalLink } from "lucide-react";
import { exploreTree, listRefs, buildBreadcrumbs } from "@/lib/explorer-api";
import BreadcrumbNav from "@/components/explorer/BreadcrumbNav";
import FileBrowser from "@/components/explorer/FileBrowser";
import CodeViewer from "@/components/explorer/CodeViewer";
import BranchSelector from "@/components/explorer/BranchSelector";

// ── Route params ─────────────────────────────────────────────

interface PageParams {
  owner: string;
  repo: string;
  revision: string;
  path?: string[];
}

interface PageProps {
  params: Promise<PageParams>;
}

// ── Metadata ─────────────────────────────────────────────────

export async function generateMetadata({
  params,
}: PageProps): Promise<Metadata> {
  const { owner, repo, revision, path } = await params;
  const filePath = path?.join("/") ?? "";
  return {
    title: `${filePath || "/"} · ${repo} @ ${revision} — SHINOBI`,
    description: `Explorateur de code pour ${owner}/${repo} à la révision ${revision}`,
  };
}

// ── Page ─────────────────────────────────────────────────────

export default async function ExplorerPage({ params }: PageProps) {
  const { owner, repo, revision, path } = await params;
  const filePath = path?.join("/") ?? "";

  // 1. Fetch parallèle : arborescence + refs
  const [explorerData, refsData] = await Promise.allSettled([
    exploreTree(owner, repo, revision, filePath),
    listRefs(owner, repo),
  ]);

  // 2. Refs (branches/tags)
  const refs =
    refsData.status === "fulfilled"
      ? refsData.value
      : { branches: [], tags: [], total: 0 };

  // 3. Breadcrumb
  const crumbs = buildBreadcrumbs(owner, repo, revision, filePath);

  // 4. Gestion erreur API
  if (explorerData.status === "rejected") {
    return (
      <div className="explorer-error">
        <h1>Erreur de navigation</h1>
        <p>{String(explorerData.reason)}</p>
        <Link href={`/${owner}/${repo}`} className="btn-secondary">
          ← Retour au dépôt
        </Link>
      </div>
    );
  }

  const data = explorerData.value;

  return (
    <div className="explorer-page">
      {/* ── Header ── */}
      <div className="explorer-header">
        <div className="explorer-header-top">
          <Link href={`/${owner}/${repo}`} className="repo-home-link">
            <span className="repo-owner">{owner}</span>
            <span className="repo-sep">/</span>
            <span className="repo-name">{repo}</span>
          </Link>
          <Link
            href={`/${owner}/${repo}/operations`}
            className="ops-link"
            title="Voir les opérations"
          >
            <ExternalLink size={14} />
            <span>Opérations</span>
          </Link>
        </div>

        <div className="explorer-controls">
          {/* BranchSelector : Client Component */}
          <BranchSelector
            owner={owner}
            repo={repo}
            currentRevision={revision}
            currentPath={filePath}
            branches={refs.branches}
            tags={refs.tags}
          />

          {/* Révision courante */}
          <div className="revision-badge">
            <GitBranch size={12} />
            <span className="revision-sha">
              {revision.length === 40
                ? revision.slice(0, 7) + "…"
                : revision}
            </span>
          </div>
        </div>

        {/* Breadcrumb */}
        <BreadcrumbNav crumbs={crumbs} />
      </div>

      {/* ── Contenu principal ── */}
      <main className="explorer-main">
        {data.kind === "directory" ? (
          <FileBrowser
            owner={owner}
            repo={repo}
            revision={revision}
            entries={data.entries}
          />
        ) : (
          <CodeViewer
            path={data.path}
            contentB64={data.content_b64}
            language={data.language}
            isText={data.is_text}
            size={data.size}
          />
        )}
      </main>
    </div>
  );
}
