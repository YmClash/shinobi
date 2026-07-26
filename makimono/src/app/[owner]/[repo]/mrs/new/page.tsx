"use client";

// ═══════════════════════════════════════════════════════════════
// /[owner]/[repo]/mrs/new — Create Merge Request (Phase 26B)
// Branch selectors + pre-diff preview + form submission
// ═══════════════════════════════════════════════════════════════

import { useCallback, useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { useAuth } from "@/hooks/use-auth";
import { createMergeRequest, getDiffBetween, type MrDiffResponse } from "@/lib/mr-api";
import { listRefs, type RefsResponse } from "@/lib/explorer-api";
import { emitMrChanged } from "@/hooks/use-mr";

export default function NewMrPage() {
  const params = useParams<{ owner: string; repo: string }>();
  const router = useRouter();
  const { user } = useAuth();

  // Form state
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [sourceBranch, setSourceBranch] = useState("");
  const [targetBranch, setTargetBranch] = useState("main");
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  // Branch data
  const [refs, setRefs] = useState<RefsResponse | null>(null);
  const [refsLoading, setRefsLoading] = useState(true);

  // Pre-diff preview
  const [preDiff, setPreDiff] = useState<MrDiffResponse | null>(null);
  const [preDiffLoading, setPreDiffLoading] = useState(false);
  const [preDiffError, setPreDiffError] = useState<string | null>(null);

  // Load branches on mount
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

  // Pre-diff when both branches are selected
  const loadPreDiff = useCallback(async () => {
    if (!sourceBranch || !targetBranch || sourceBranch === targetBranch) {
      setPreDiff(null);
      return;
    }
    setPreDiffLoading(true);
    setPreDiffError(null);
    try {
      const data = await getDiffBetween(
        params.owner,
        params.repo,
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
  }, [params.owner, params.repo, sourceBranch, targetBranch]);

  useEffect(() => {
    const timeout = setTimeout(loadPreDiff, 500);
    return () => clearTimeout(timeout);
  }, [loadPreDiff]);

  // Submit
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) {
      setFormError("Title is required");
      return;
    }
    if (!sourceBranch) {
      setFormError("Source branch is required");
      return;
    }
    if (sourceBranch === targetBranch) {
      setFormError("Source and target branches must be different");
      return;
    }

    setSubmitting(true);
    setFormError(null);
    try {
      const mr = await createMergeRequest(params.owner, params.repo, {
        title: title.trim(),
        description: description.trim() || undefined,
        source_branch: sourceBranch,
        target_branch: targetBranch,
      });
      emitMrChanged();
      router.push(`/${params.owner}/${params.repo}/mrs/${mr.number}`);
    } catch (err) {
      setFormError(
        err instanceof Error ? err.message : "Failed to create merge request",
      );
      setSubmitting(false);
    }
  };

  // Redirect if not authenticated
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
                disabled={submitting}
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
            <label className="mr-form-label">Target Branch</label>
            {refsLoading ? (
              <div className="mr-skeleton mr-skeleton-sm animate-shimmer" />
            ) : (
              <select
                className="mr-form-select"
                value={targetBranch}
                onChange={(e) => setTargetBranch(e.target.value)}
                disabled={submitting}
              >
                {refs?.branches.map((b) => (
                  <option key={b.name} value={b.name}>
                    {b.name}
                  </option>
                ))}
              </select>
            )}
          </div>
        </div>

        {/* ── Pre-Diff Preview ─────────────────────── */}
        {(preDiffLoading || preDiff || preDiffError) && (
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
            disabled={submitting}
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
            disabled={submitting}
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
            disabled={submitting}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="mr-action-btn mr-action-merge"
            disabled={submitting || !title.trim() || !sourceBranch}
          >
            {submitting ? (
              <>
                <span className="mr-action-spinner" /> Creating…
              </>
            ) : (
              "⚔️ Create Merge Request"
            )}
          </button>
        </div>
      </form>
    </div>
  );
}
