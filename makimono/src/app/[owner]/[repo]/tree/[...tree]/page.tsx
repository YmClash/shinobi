// ═══════════════════════════════════════════════════════════════
// SHINOBI — Code Explorer Page v3
// Route: /[owner]/[repo]/tree/[...tree]
//
// Catch-all route: les segments après /tree/ sont combinés puis
// résolus dynamiquement en (revision, path) via les refs connues.
// Supporte les branches avec "/" (ex: feature/login).
//
// Layout 2 colonnes :
//   - Gauche  : FileBrowser (répertoire) ou CodeViewer (fichier)
//   - Droite  : Sidebar "À propos" avec métadonnées repo + Oracle
// ═══════════════════════════════════════════════════════════════

import React from "react";
import type { Metadata } from "next";
import Link from "next/link";
import {
  Search,
  Download,
  GitBranch,
  History,
  BrainCircuit,
  FileCode,
  ExternalLink,
} from "lucide-react";
import { exploreTree, listRefs, buildBreadcrumbs, resolveRevisionAndPath } from "@/lib/explorer-api";
import { getRepository, listOperations, getOperationReviews, buildRepoPrefix } from "@/lib/api";
import BreadcrumbNav from "@/components/explorer/BreadcrumbNav";
import BranchSelector from "@/components/explorer/BranchSelector";
import FileBrowser from "@/components/explorer/FileBrowser";
import ExplorerFileClient from "@/components/explorer/ExplorerFileClient";
import DeleteRepoButton from "@/components/forge/delete-repo-button";
import CloneDropdown from "@/components/explorer/CloneDropdown";
import { ForkButton } from "@/components/repo/fork-button";

// ── Route params ─────────────────────────────────────────────

interface PageParams {
  owner: string;
  repo: string;
  tree: string[];  // catch-all: ["main"] ou ["feature","login","src","main.rs"]
}

interface PageProps {
  params: Promise<PageParams>;
}

// ── Metadata ─────────────────────────────────────────────────

export async function generateMetadata({
  params,
}: PageProps): Promise<Metadata> {
  const { owner, repo, tree } = await params;
  // Fallback metadata — la résolution exacte se fait dans la page
  const display = tree.join("/");
  return {
    title: `${display || "/"} · ${repo} — SHINOBI`,
    description: `Explorateur de code pour ${owner}/${repo}`,
  };
}

// ── Helpers ──────────────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    const diff = (Date.now() - d.getTime()) / 1000;
    if (diff < 60) return "il y a quelques secondes";
    if (diff < 3600) return `il y a ${Math.floor(diff / 60)} min`;
    if (diff < 86400) return `il y a ${Math.floor(diff / 3600)} h`;
    if (diff < 604800) return `il y a ${Math.floor(diff / 86400)} jours`;
    return d.toLocaleDateString("fr-FR");
  } catch {
    return iso;
  }
}

// Détecter le langage dominant depuis les fichiers listés
function detectPrimaryLanguage(entries: { name: string }[]): string | null {
  const counts: Record<string, number> = {};
  for (const e of entries) {
    const ext = e.name.split(".").pop()?.toLowerCase();
    if (ext) counts[ext] = (counts[ext] ?? 0) + 1;
  }
  const langMap: Record<string, string> = {
    rs: "Rust",
    ts: "TypeScript",
    tsx: "TypeScript/React",
    js: "JavaScript",
    py: "Python",
    go: "Go",
    java: "Java",
  };
  const top = Object.entries(counts).sort((a, b) => b[1] - a[1])[0];
  return top ? (langMap[top[0]] ?? top[0].toUpperCase()) : null;
}

// ── Page ─────────────────────────────────────────────────────

export default async function ExplorerPage({ params }: PageProps) {
  const { owner, repo, tree } = await params;

  // ── Fetch refs en premier (nécessaire pour résoudre la revision) ──
  const refsData = await listRefs(owner, repo).catch(() => ({
    branches: [] as { name: string; target: string }[],
    tags: [] as { name: string; target: string }[],
    total: 0,
  }));

  // ── Résolution revision/path depuis les segments catch-all ──
  const knownRefs = [
    ...refsData.branches.map((b) => b.name),
    ...refsData.tags.map((t) => t.name),
  ];
  const { revision, path: filePath } = resolveRevisionAndPath(tree, knownRefs);

  // ── Fetch parallèle ──────────────────────────────────────────
  const [explorerData, repoData, opsData] =
    await Promise.allSettled([
      exploreTree(owner, repo, revision, filePath),
      getRepository(owner, repo),
      listOperations(buildRepoPrefix(owner, repo), 1), // dernier commit
    ]);

  // ── Refs ─────────────────────────────────────────────────────
  const refs = refsData;

  // ── Métadonnées repo ─────────────────────────────────────────
  const repoMeta =
    repoData.status === "fulfilled" ? repoData.value : null;

  // ── Dernier commit (opération) ────────────────────────────────
  const lastOp =
    opsData.status === "fulfilled" && opsData.value.operations.length > 0
      ? opsData.value.operations[0]
      : null;

  // ── Oracle Score — fetch reviews du dernier commit (Phase 14) ──
  let oracleScore: number | null = null;
  let latestReviewSummary: string | null = null;
  let latestReviewModel: string | null = null;

  if (lastOp) {
    try {
      const prefix = buildRepoPrefix(owner, repo);
      const reviewsData = await getOperationReviews(prefix, lastOp.id);
      if (reviewsData.reviews.length > 0) {
        const bestReview = reviewsData.reviews[0];
        oracleScore = bestReview.score !== null
          ? Math.round(bestReview.score * 100)
          : null;
        latestReviewSummary = bestReview.summary;
        latestReviewModel = bestReview.model;
      }
    } catch {
      // Graceful degradation — pas de review disponible
    }
  }

  const latestCommit = lastOp
    ? {
      hash: lastOp.content_id,
      message: lastOp.description,
      author: lastOp.author_id.slice(0, 8), // short UUID as author
      date: formatDate(lastOp.created_at),
      oracleScore,
    }
    : undefined;

  // ── Breadcrumb ────────────────────────────────────────────────
  const crumbs = buildBreadcrumbs(owner, repo, revision, filePath);

  // ── Erreur VCS ────────────────────────────────────────────────
  if (explorerData.status === "rejected") {
    const errMsg = String(explorerData.reason).toLowerCase();

    // Dépôt vide : révision introuvable (pas de commits encore)
    const isEmpty =
      errMsg.includes("introuvable") ||
      errMsg.includes("not found") ||
      errMsg.includes("404") ||
      errMsg.includes("vcsError") ||
      errMsg.includes("vcserror") ||
      errMsg.includes("500");

    if (isEmpty) {
      // URL HTTP Git pour les commandes de clone
      const httpUrl = `http://localhost:3000/${owner}/${repo}.git`;

      return (
        <div className="ex-page">
          {/* Header repo même pour un dépôt vide */}
          <header className="ex-repo-header">
            <div className="ex-repo-header-inner">
              <div className="ex-repo-title">
                <Link href={`/${owner}/${repo}`} className="ex-repo-link">
                  <span className="ex-owner">{owner}</span>
                  <span className="ex-sep">/</span>
                  <span className="ex-reponame">{repo}</span>
                </Link>
                {repoData.status === "fulfilled" && (
                  <span className="ex-visibility-badge">
                    {repoData.value.visibility === "public" ? "Public" : "Privé"}
                  </span>
                )}
              </div>
            </div>
          </header>

          {/* État vide */}
          <div className="ex-empty-repo">
            <div className="ex-empty-icon" aria-hidden="true">📦</div>
            <h1 className="ex-empty-title">Dépôt vide</h1>
            <p className="ex-empty-subtitle">
              Ce dépôt n&apos;a pas encore de commits. Effectuez un premier push pour commencer.
            </p>

            <div className="ex-empty-steps">
              {/* Étape 1 : Créer un dépôt local */}
              <div className="ex-empty-step">
                <div className="ex-step-header">
                  <span className="ex-step-num">1</span>
                  <span className="ex-step-label">Créer un dépôt local</span>
                </div>
                <div className="ex-code-block">
                  <pre>{`git init mon-projet
cd mon-projet
git checkout -b main`}</pre>
                </div>
              </div>

              {/* Étape 2 : Connecter à SHINOBI */}
              <div className="ex-empty-step">
                <div className="ex-step-header">
                  <span className="ex-step-num">2</span>
                  <span className="ex-step-label">Connecter à la Forge</span>
                </div>
                <div className="ex-code-block">
                  <pre>{`git remote add origin ${httpUrl}`}</pre>
                </div>
              </div>

              {/* Étape 3 : Authentification PAT (Phase 19A-Git) */}
              <div className="ex-empty-step">
                <div className="ex-step-header">
                  <span className="ex-step-num">3</span>
                  <span className="ex-step-label">Authentification (PAT)</span>
                </div>
                <p className="ex-step-desc">
                  Créez un <a href="/settings/tokens" className="ex-step-link">Personal Access Token</a> puis configurez Git :
                </p>
                <div className="ex-code-block">
                  <pre>{`# Option A : Git credential store (persistant)
git config --global credential.helper store
# Lors du premier push, entrez :
#   Username: ${owner}
#   Password: shb_votre_token_ici

# Option B : URL avec token intégré
git remote set-url origin http://${owner}:VOTRE_PAT@localhost:3000/${owner}/${repo}.git`}</pre>
                </div>
                <p className="ex-step-hint">
                  🔑 Le token n&apos;est affiché qu&apos;une seule fois lors de sa création. Copiez-le avant de quitter la page.
                </p>
              </div>

              {/* Étape 4 : Premier push */}
              <div className="ex-empty-step">
                <div className="ex-step-header">
                  <span className="ex-step-num">4</span>
                  <span className="ex-step-label">Premier commit &amp; push</span>
                </div>
                <div className="ex-code-block">
                  <pre>{`echo "# ${repo}" > README.md
git add .
git commit -m "feat: initial commit"
git push -u origin main`}</pre>
                </div>
              </div>
            </div>

            <p className="ex-empty-hint">
              💡 Après le push, cette page se mettra à jour automatiquement.
            </p>
          </div>
        </div>
      );
    }

    // Vraie erreur technique
    return (
      <div className="ex-error">
        <h1>Erreur de navigation</h1>
        <p>{String(explorerData.reason)}</p>
        <Link href={`/${owner}/${repo}`} className="ex-error-back">
          ← Retour au dépôt
        </Link>
      </div>
    );
  }

  const data = explorerData.value;
  const isFile = data.kind === "file";
  const entries = data.kind === "directory" ? data.entries : [];
  const primaryLang =
    repoMeta === null ? detectPrimaryLanguage(entries) : null;

  return (
    <div className="ex-page">
      {/* ══════════ REPO HEADER ══════════════════════════════ */}
      <header className="ex-repo-header">
        <div className="ex-repo-header-inner">
          {/* Titre repo */}
          <div className="ex-repo-title">
            <Link href={`/${owner}/${repo}`} className="ex-repo-link">
              <span className="ex-owner">{owner}</span>
              <span className="ex-sep">/</span>
              <span className="ex-reponame">{repo}</span>
            </Link>
            {repoMeta && (
              <span className="ex-visibility-badge">
                {repoMeta.visibility === "public" ? "Public" : "Privé"}
              </span>
            )}
          </div>

          {/* Actions */}
          <div className="ex-repo-actions">
            <Link
              href={`/${owner}/${repo}/operations`}
              className="ex-btn ex-btn-ghost"
            >
              <ExternalLink size={14} />
              <span>Opérations</span>
            </Link>
            <button className="ex-btn ex-btn-ghost">
              <Search size={14} />
              <span>Rechercher</span>
            </button>
            {repoMeta && (
              <ForkButton
                owner={owner}
                repo={repo}
                repoOwnerId={repoMeta.owner_id}
                forkCount={repoMeta.fork_count ?? 0}
              />
            )}
            <CloneDropdown owner={owner} repo={repo} />
            {repoMeta && (
              <DeleteRepoButton
                owner={owner}
                repo={repo}
                displayName={repoMeta.display_name}
                ownerId={repoMeta.owner_id}
              />
            )}
          </div>
        </div>
      </header>

      {/* ══════════ CONTROLS BAR ═════════════════════════════ */}
      <div className="ex-controls-bar">
        <div className="ex-controls-left">
          {/* BranchSelector */}
          <BranchSelector
            owner={owner}
            repo={repo}
            currentRevision={revision}
            currentPath={filePath}
            branches={refs.branches}
            tags={refs.tags}
          />

          {/* Breadcrumb */}
          <BreadcrumbNav crumbs={crumbs} repoName={repo} />
        </div>

        <div className="ex-controls-right">
          <div className="ex-stat" style={{ cursor: 'pointer' }}>
            <GitBranch size={13} />
            <a href={`/${owner}/${repo}/bookmarks`} style={{ textDecoration: 'none', color: 'inherit' }}>
              {refs.branches.length} Bookmarks
            </a>
          </div>
          <div className="ex-stat" style={{ cursor: 'pointer' }}>
            <History size={13} />
            <a href={`/${owner}/${repo}/commits`} style={{ textDecoration: 'none', color: 'inherit' }}>
              {opsData.status === "fulfilled"
                ? `${opsData.value.total_count ?? opsData.value.count} Commits`
                : "…"}
            </a>
          </div>
        </div>
      </div>

      {/* ══════════ BODY — 2 colonnes ════════════════════════ */}
      <div className="ex-body">
        {/* ── Colonne principale ── */}
        <main className="ex-main">
          {isFile ? (
            <ExplorerFileClient
              path={data.path}
              contentB64={data.content_b64}
              language={data.language}
              isText={data.is_text}
              size={data.size}
              owner={owner}
              repo={repo}
            />
          ) : (
            <FileBrowser
              owner={owner}
              repo={repo}
              revision={revision}
              entries={entries}
              latestCommit={latestCommit}
              branchCount={refs.branches.length}
              commitCount={
                opsData.status === "fulfilled" ? (opsData.value.total_count ?? opsData.value.count) : 0
              }
            />
          )}
        </main>

        {/* ── Sidebar droite ── */}
        <aside className="ex-sidebar">
          {/* À propos */}
          <div className="ex-sidebar-card">
            <h3 className="ex-sidebar-title">À propos</h3>
            <p className="ex-sidebar-desc">
              {repoMeta?.description ??
                `Dépôt ${owner}/${repo} géré par la Forge SHINOBI.`}
            </p>

            <div className="ex-sidebar-divider" />

            <ul className="ex-sidebar-meta">
              <li className="ex-sidebar-item ex-sidebar-item-oracle">
                <BrainCircuit size={15} className="ex-sidebar-icon-oracle" />
                <span>Oracle Tensai actif</span>
              </li>
              <li className="ex-sidebar-item">
                <FileCode size={15} className="ex-sidebar-icon-code" />
                <Link
                  href={`/${owner}/${repo}/tree/${revision}/README.md`}
                  className="ex-sidebar-link"
                >
                  README
                </Link>
              </li>
              {primaryLang && (
                <li className="ex-sidebar-item">
                  <span className="ex-lang-dot" />
                  <span>{primaryLang} 100%</span>
                </li>
              )}
              {repoMeta?.default_branch && (
                <li className="ex-sidebar-item">
                  <GitBranch size={15} className="ex-sidebar-icon-branch" />
                  <span>Branche par défaut : {repoMeta.default_branch}</span>
                </li>
              )}
            </ul>
          </div>

          {/* Oracle Score (Phase 14) */}
          {oracleScore !== null && (
            <div className="ex-sidebar-card ex-sidebar-card-oracle">
              <h3 className="ex-sidebar-title">Oracle Tensai</h3>
              <div className="ex-oracle-score-display">
                <div className="ex-oracle-gauge">
                  <span className={`ex-oracle-value ${oracleScore >= 80 ? "ex-oracle-good" :
                    oracleScore >= 60 ? "ex-oracle-mid" : "ex-oracle-low"
                    }`}>
                    {oracleScore}
                  </span>
                  <span className="ex-oracle-max">/100</span>
                </div>
                {latestReviewModel && (
                  <span className="ex-oracle-model">{latestReviewModel}</span>
                )}
              </div>
              {latestReviewSummary && (
                <p className="ex-sidebar-desc ex-oracle-summary">
                  {latestReviewSummary}
                </p>
              )}
            </div>
          )}

          {/* Dernière révision */}
          {latestCommit && (
            <div className="ex-sidebar-card ex-sidebar-card-commit">
              <h3 className="ex-sidebar-title">Dernière opération</h3>
              <p className="ex-sidebar-commit-msg">{latestCommit.message}</p>
              <div className="ex-sidebar-commit-meta">
                <span className="ex-sidebar-commit-hash">
                  {latestCommit.hash.slice(0, 8)}
                </span>
                <span className="ex-sidebar-commit-date">
                  {latestCommit.date}
                </span>
              </div>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
