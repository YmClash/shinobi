"use client";

// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo]/mrs/new — Create Merge Request (Phase 26B + 37E-UI)
// Branch selectors + pre-diff preview + cross-repo fork→parent flow
// Uses React 19 useTransition for idiomatic pending state management
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useState, useTransition } from "react";
import { useParams, useRouter } from "next/navigation";
import { useAuth } from "@/hooks/use-auth";
import { createMergeRequest, getDiffBetween, type MrDiffResponse } from "@/lib/mr-api";
import { listRefs, type RefsResponse } from "@/lib/explorer-api";
import { getRepository, type Repository } from "@/lib/api";
import { emitMrChanged } from "@/hooks/use-mr";

export default function NewMrPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const router = useRouter();
  const { user } = useAuth();

  // ── Repo metadata (fork detection) ───────────────────────
  const [repoData, setRepoData] = useState<Repository | null>(null);
  const [repoLoading, setRepoLoading] = useState(true);

  // ── Cross-repo toggle (Phase 37E-UI) ────────────────────
  const [crossRepo, setCrossRepo] = useState(false);
  const [parentRefs, setParentRefs] = useState<RefsResponse | null>(null);
  const [parentRefsLoading, setParentRefsLoading] = useState(false);
  const [parentRefsError, setParentRefsError] = useState<string | null>(null);

  // ── Form state ──────────────────────────────────────────
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [sourceBranch, setSourceBranch] = useState("");
  const [targetBranch, setTargetBranch] = useState("main");
  const [formError, setFormError] = useState<string | null>(null);

  // ── React 19 useTransition for submit ───────────────────
  const [isPending, startTransition] = useTransition();

  // ── Branch data (fork's own branches) ───────────────────
  const [refs, setRefs] = useState<RefsResponse | null>(null);
  const [refsLoading, setRefsLoading] = useState(true);

  // ── Pre-diff preview ────────────────────────────────────
  const [preDiff, setPreDiff] = useState<MrDiffResponse | null>(null);
  const [preDiffLoading, setPreDiffLoading] = useState(false);
  const [preDiffError, setPreDiffError] = useState<string | null>(null);

  // Derived fork info
  const isFork = Boolean(repoData?.forked_from_id);
  const parentOwner = repoData?.forked_from_owner ?? "";
  const parentName = repoData?.forked_from_name ?? "";

  // ── 1. Fetch repo metadata (to detect fork) ────────────
  useEffect(() => {
    setRepoLoading(true);
    getRepository(params.owner, params.repo)
      .then((data) => {
        setRepoData(data);
        // Auto-enable cross-repo if this is a fork
        if (data.forked_from_id) {
          setCrossRepo(true);
        }
      })
      .catch(() => setRepoData(null))
      .finally(() => setRepoLoading(false));
  }, [params.owner, params.repo]);

  // ── 2. Fetch fork's own branches ───────────────────────
  useEffect(() => {
    listRefs(params.owner, params.repo)
      .then((data) => {
        setRefs(data);
        // Auto-select first non-main branch as source
        const firstNonMain = data.branches.find(
          (b) => b.name !== "main" && b.name !== "master",
        );
        if (firstNonMain) setSourceBranch(firstNonMain.name);
      })
      .catch(() => setRefs(null))
      .finally(() => setRefsLoading(false));
  }, [params.owner, params.repo]);

  // ── 3. Fetch parent repo branches (when cross-repo ON) ─
  useEffect(() => {
    if (!crossRepo || !parentOwner || !parentName) {
      setParentRefs(null);
      setParentRefsError(null);
      return;
    }

    setParentRefsLoading(true);
    setParentRefsError(null);
    listRefs(parentOwner, parentName)
      .then((data) => {
        setParentRefs(data);
        // Auto-select default branch of parent as target
        const hasMain = data.branches.some((b) => b.name === "main");
        const hasMaster = data.branches.some((b) => b.name === "master");
        if (hasMaster) setTargetBranch("master");
        else if (hasMain) setTargetBranch("main");
        else if (data.branches.length > 0) setTargetBranch(data.branches[0].name);
      })
      .catch((err) => {
        setParentRefsError(
          err instanceof Error
            ? `Cannot reach parent repository: ${err.message}`
            : "Parent repository unreachable",
        );
        setParentRefs(null);
      })
      .finally(() => setParentRefsLoading(false));
  }, [crossRepo, parentOwner, parentName]);

  // ── 4. Reset target branch when toggling cross-repo ────
  useEffect(() => {
    if (!crossRepo && refs) {
      // Switched to intra-repo → reset target to fork's default
      const hasMain = refs.branches.some((b) => b.name === "main");
      setTargetBranch(hasMain ? "main" : refs.branches[0]?.name ?? "main");
    }
  }, [crossRepo, refs]);

  // The branches to show in the Target dropdown
  const targetBranches = crossRepo && parentRefs ? parentRefs.branches : refs?.branches ?? [];

  // ── 5. Pre-diff when both branches are selected ────────
  const preDiffOwner = crossRepo ? parentOwner : params.owner;
  const preDiffRepo = crossRepo ? parentName : params.repo;

  const loadPreDiff = useCallback(async () => {
    if (!sourceBranch || !targetBranch || sourceBranch === targetBranch) {
      setPreDiff(null);
      return;
    }
    // For cross-repo, skip pre-diff (branches are on different repos)
    if (crossRepo) {
      setPreDiff(null);
      return;
    }
    setPreDiffLoading(true);
    setPreDiffError(null);
    try {
      const data = await getDiffBetween(
        preDiffOwner,
        preDiffRepo,
        sourceBranch,
        targetBranch,
      );
      setPreDiff(data);
    } catch (err) {
      setPreDiffError(err instanceof Error ? err.message : "Failed to load diff");
      setPreDiff(null);
    } finally {
      setPreDiffLoading(false);
    }
  }, [preDiffOwner, preDiffRepo, sourceBranch, targetBranch, crossRepo]);

  useEffect(() => {
    const timeout = setTimeout(loadPreDiff, 500);
    return () => clearTimeout(timeout);
  }, [loadPreDiff]);

  // ── 6. Submit — useTransition pattern (Vegapunk) ───────
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) {
      setFormError("Title is required");
      return;
    }
    if (!sourceBranch) {
      setFormError("Source branch is required");
      return;
    }
    if (sourceBranch === targetBranch && !crossRepo) {
      setFormError("Source and target branches must be different");
      return;
    }
    if (crossRepo && parentRefsError) {
      setFormError("Cannot create cross-repo MR: parent repository unreachable");
      return;
    }

    setFormError(null);

    startTransition(async () => {
      try {
        // Cross-repo: POST to parent repo with source_repo ref
        // Intra-repo: POST to current repo (classic behavior)
        const targetOwner = crossRepo ? parentOwner : params.owner;
        const targetRepoName = crossRepo ? parentName : params.repo;

        const mr = await createMergeRequest(targetOwner, targetRepoName, {
          title: title.trim(),
          description: description.trim() || undefined,
          source_branch: sourceBranch,
          target_branch: targetBranch,
          ...(crossRepo
            ? { source_repo: { owner: params.owner, name: params.repo } }
            : {}),
        });

        emitMrChanged();
        // Redirect to MR on the target repo
        router.push(`/${targetOwner}/${targetRepoName}/mrs/${mr.number}`);
      } catch (err) {
        setFormError(
          err instanceof Error ? err.message : "Failed to create merge request",
        );
      }
    });
  };

  // ── Redirect if not authenticated ──────────────────────
  if (!user) {
    return (
      <div className="mr-page-container">
        <div className="mr-error">
          <span className="mr-error-icon">🔒</span>
          <span>You must be logged in to create a merge request.</span>
        </div>
      </div>
    );
  }

  return (
    <div className="mr-page-container">
      <div className="mr-page-header">
        <h1 className="mr-page-title">
          <span className="mr-page-icon">⚔️</span>
          New Merge Request
        </h1>
      </div>

      <form onSubmit={handleSubmit} className="mr-create-form animate-fade-in-up">
        {/* ── Cross-Repo Banner (Phase 37E-UI) ──────── */}
        {isFork && !repoLoading && (
          <div className="mr-crossrepo-banner">
            <div className="mr-crossrepo-header">
              <div className="mr-crossrepo-title">
                <span className="mr-crossrepo-title-icon">🕳️</span>
                Cross-Repository Merge Request
              </div>
              <div className="mr-crossrepo-toggle-wrap">
                <label className="mr-crossrepo-toggle-label" htmlFor="cross-repo-toggle">
                  Merge into parent
                </label>
                <input
                  id="cross-repo-toggle"
                  type="checkbox"
                  className="mr-crossrepo-toggle"
                  checked={crossRepo}
                  onChange={(e) => setCrossRepo(e.target.checked)}
                  disabled={isPending}
                />
              </div>
            </div>

            {crossRepo && (
              <div className="mr-crossrepo-flow">
                <span className="mr-crossrepo-repo mr-crossrepo-repo-source">
                  {params.owner}/{params.repo}:{sourceBranch || "…"}
                </span>
                <span className="mr-crossrepo-arrow">→</span>
                <span className="mr-crossrepo-repo mr-crossrepo-repo-target">
                  {parentOwner}/{parentName}:{targetBranch || "…"}
                </span>
              </div>
            )}

            {!crossRepo && (
              <div className="mr-crossrepo-hint">
                Toggle on to merge changes into the parent repository
              </div>
            )}

            {crossRepo && parentRefsError && (
              <div className="mr-crossrepo-error">
                <span>⚠️</span>
                <span>{parentRefsError}</span>
              </div>
            )}
          </div>
        )}

        {/* ── Branch Selectors ──────────────────────── */}
        <div className="mr-branch-selectors">
          <div className="mr-branch-select-group">
            <label className="mr-form-label">Source Branch</label>
            {refsLoading ? (
              <div className="mr-skeleton mr-skeleton-sm animate-shimmer" />
            ) : (
              <select
                className="mr-form-select"
                value={sourceBranch}
                onChange={(e) => setSourceBranch(e.target.value)}
                disabled={isPending}
              >
                <option value="">Select a branch…</option>
                {refs?.branches.map((b) => (
                  <option key={b.name} value={b.name}>
                    {b.name}
                  </option>
                ))}
              </select>
            )}
          </div>

          <div className="mr-branch-arrow-lg">→</div>

          <div className="mr-branch-select-group">
            <label className="mr-form-label">
              Target Branch
              {crossRepo && (
                <span className="mr-form-optional"> ({parentOwner}/{parentName})</span>
              )}
            </label>
            {(refsLoading || parentRefsLoading) ? (
              <div className="mr-skeleton mr-skeleton-sm animate-shimmer" />
            ) : (
              <select
                className="mr-form-select"
                value={targetBranch}
                onChange={(e) => setTargetBranch(e.target.value)}
                disabled={isPending || (crossRepo && !!parentRefsError)}
              >
                {targetBranches.map((b) => (
                  <option key={b.name} value={b.name}>
                    {b.name}
                  </option>
                ))}
              </select>
            )}
          </div>
        </div>

        {/* ── Pre-Diff Preview (intra-repo only) ─────── */}
        {!crossRepo && (preDiffLoading || preDiff || preDiffError) && (
          <div className="mr-prediff-section">
            <h3 className="mr-section-title">
              📄 Changes Preview
              {preDiff && (
                <span className="mr-prediff-count">
                  {preDiff.total_files} file{preDiff.total_files !== 1 ? "s" : ""}
                </span>
              )}
            </h3>
            {preDiffLoading && (
              <div className="mr-skeleton animate-shimmer" />
            )}
            {preDiffError && (
              <div className="mr-prediff-error">{preDiffError}</div>
            )}
            {preDiff && preDiff.files.length === 0 && (
              <div className="mr-prediff-empty">
                No differences between these branches.
              </div>
            )}
            {preDiff &&
              preDiff.files.map((file) => (
                <div key={file.path} className="mr-prediff-file">
                  <span
                    className={`mr-diff-status mr-diff-status-${file.status}`}
                  >
                    {file.status === "added"
                      ? "A"
                      : file.status === "deleted"
                        ? "D"
                        : "M"}
                  </span>
                  <span className="mr-diff-file-path">{file.path}</span>
                  <span className="mr-diff-stats">
                    {file.additions > 0 && (
                      <span className="mr-diff-additions">
                        +{file.additions}
                      </span>
                    )}
                    {file.deletions > 0 && (
                      <span className="mr-diff-deletions">
                        -{file.deletions}
                      </span>
                    )}
                  </span>
                </div>
              ))}
          </div>
        )}

        {/* ── Title ────────────────────────────────── */}
        <div className="mr-form-group">
          <label className="mr-form-label" htmlFor="mr-title">
            Title
          </label>
          <input
            id="mr-title"
            type="text"
            className="mr-form-input"
            placeholder="Brief description of changes…"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            disabled={isPending}
            autoFocus
          />
        </div>

        {/* ── Description ──────────────────────────── */}
        <div className="mr-form-group">
          <label className="mr-form-label" htmlFor="mr-description">
            Description <span className="mr-form-optional">(optional)</span>
          </label>
          <textarea
            id="mr-description"
            className="mr-form-textarea"
            placeholder="Detailed explanation of your changes…"
            rows={6}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            disabled={isPending}
          />
        </div>

        {/* ── Error ────────────────────────────────── */}
        {formError && (
          <div className="mr-action-error animate-fade-in-up">
            <span className="mr-action-error-icon">⚠️</span>
            <span className="mr-action-error-text">{formError}</span>
          </div>
        )}

        {/* ── Submit ───────────────────────────────── */}
        <div className="mr-form-actions">
          <button
            type="button"
            className="mr-action-btn mr-action-close"
            onClick={() => router.back()}
            disabled={isPending}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="mr-action-btn mr-action-merge"
            disabled={isPending || !title.trim() || !sourceBranch || (crossRepo && !!parentRefsError)}
          >
            {isPending ? (
              <>
                <span className="mr-action-spinner" /> Creating…
              </>
            ) : crossRepo ? (
              "🕳️ Create Cross-Repo MR"
            ) : (
              "⚔️ Create Merge Request"
            )}
          </button>
        </div>
      </form>
    </div>
  );
}
