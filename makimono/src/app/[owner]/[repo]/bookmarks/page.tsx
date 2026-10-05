// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo]/bookmarks — Bookmarks (Branches & Tags) Page
// Phase 19B+ — Vue complète des références d'un dépôt
// ═══════════════════════════════════════════════════════════════

import { notFound } from "next/navigation";
import { listRefs, type RefItem } from "@/lib/explorer-api";
import { getRepository } from "@/lib/api";
import type { Metadata } from "next";
import Link from "next/link";

interface PageProps {
  params: Promise<{ owner: string; repo: string }>;
}

export async function generateMetadata({
  params,
}: PageProps): Promise<Metadata> {
  const { owner, repo } = await params;
  return {
    title: `Bookmarks · ${owner}/${repo} — SHINOBI`,
    description: `Branches et tags du dépôt ${owner}/${repo}`,
  };
}

export default async function BookmarksPage({ params }: PageProps) {
  const { owner, repo } = await params;

  let refs;
  let defaultBranch = "main";

  try {
    const [refsData, repoMeta] = await Promise.all([
      listRefs(owner, repo),
      getRepository(owner, repo),
    ]);
    refs = refsData;
    defaultBranch = repoMeta.default_branch || "main";
  } catch {
    notFound();
  }

  return (
    <div className="bk-page">
      {/* ── Header ─────────────────────────────── */}
      <header className="bk-header">
        <div className="bk-header-left">
          <Link href={`/${owner}/${repo}`} className="bk-back-link">
            ← {owner}/{repo}
          </Link>
          <h1 className="bk-title">
            <span className="bk-title-icon">🔖</span>
            Bookmarks
          </h1>
          <p className="bk-subtitle">
            {refs.branches.length} branche{refs.branches.length !== 1 ? "s" : ""} · {refs.tags.length} tag{refs.tags.length !== 1 ? "s" : ""}
          </p>
        </div>
      </header>

      {/* ── Branches ───────────────────────────── */}
      <section className="bk-section">
        <h2 className="bk-section-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" />
          </svg>
          Branches
          <span className="bk-count">{refs.branches.length}</span>
        </h2>

        {refs.branches.length === 0 ? (
          <div className="bk-empty">Aucune branche détectée</div>
        ) : (
          <div className="bk-list">
            {refs.branches.map((branch: RefItem) => (
              <BookmarkRow
                key={branch.name}
                refItem={branch}
                isDefault={branch.name === defaultBranch}
                owner={owner}
                repo={repo}
                kind="branch"
              />
            ))}
          </div>
        )}
      </section>

      {/* ── Tags ───────────────────────────────── */}
      {refs.tags.length > 0 && (
        <section className="bk-section">
          <h2 className="bk-section-title">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
            </svg>
            Tags
            <span className="bk-count">{refs.tags.length}</span>
          </h2>

          <div className="bk-list">
            {refs.tags.map((tag: RefItem) => (
              <BookmarkRow
                key={tag.name}
                refItem={tag}
                isDefault={false}
                owner={owner}
                repo={repo}
                kind="tag"
              />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

// ── BookmarkRow ─────────────────────────────────────────────

interface BookmarkRowProps {
  refItem: RefItem;
  isDefault: boolean;
  owner: string;
  repo: string;
  kind: "branch" | "tag";
}

function BookmarkRow({ refItem, isDefault, owner, repo, kind }: BookmarkRowProps) {
  const browseHref = `/${owner}/${repo}/tree/${encodeURIComponent(refItem.name)}`;
  const shortHash = refItem.target.slice(0, 10);
  const isBranch = kind === "branch";

  return (
    <div className="bk-row">
      <div className="bk-row-left">
        <span className={`bk-row-icon ${isBranch ? "bk-row-icon-branch" : "bk-row-icon-tag"}`}>
          {isBranch ? (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" />
            </svg>
          ) : (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" /><line x1="7" y1="7" x2="7.01" y2="7" />
            </svg>
          )}
        </span>
        <Link href={browseHref} className="bk-row-name">
          {refItem.name}
        </Link>
        {isDefault && (
          <span className="bk-default-badge">default</span>
        )}
      </div>

      <div className="bk-row-right">
        <code className="bk-row-hash" title={refItem.target}>
          {shortHash}
        </code>
        <Link href={browseHref} className="bk-row-browse">
          Explorer →
        </Link>
      </div>
    </div>
  );
}
