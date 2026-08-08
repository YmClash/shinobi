"use client";

// ═══════════════════════════════════════════════════════════════
// New Issue Page — Phase 33
// /[owner]/[repo]/issues/new
// ═══════════════════════════════════════════════════════════════

import { useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { createIssue } from "@/lib/issue-api";
import { useLabels } from "@/hooks/use-issues";
import { emitIssueChanged } from "@/hooks/use-issues";
import { IssueLabelBadge } from "@/components/issue/issue-label-badge";

export default function NewIssuePage() {
  const params = useParams<{ owner: string; repo: string }>();
  const router = useRouter();

  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [selectedLabels, setSelectedLabels] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showLabelPicker, setShowLabelPicker] = useState(false);

  const { data: labelsData } = useLabels(params.owner, params.repo);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const issue = await createIssue(params.owner, params.repo, {
        title: title.trim(),
        body: body.trim() || undefined,
        label_ids: selectedLabels,
      });
      emitIssueChanged(params.owner, params.repo);
      router.push(`/${params.owner}/${params.repo}/issues/${issue.number}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Erreur inconnue");
    } finally {
      setSubmitting(false);
    }
  }

  function toggleLabel(id: string) {
    setSelectedLabels((prev) =>
      prev.includes(id) ? prev.filter((l) => l !== id) : [...prev, id]
    );
  }

  return (
    <div className="issue-new-page">
      <h1 className="issue-new-title">✨ New Issue</h1>

      <form onSubmit={handleSubmit} className="issue-form">
        {/* Title */}
        <div className="issue-form-group">
          <label htmlFor="issue-title" className="issue-form-label">
            Title
          </label>
          <input
            id="issue-title"
            type="text"
            className="issue-form-input"
            placeholder="Bug: login fails on Chrome 120"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            required
            autoFocus
          />
        </div>

        {/* Body */}
        <div className="issue-form-group">
          <label htmlFor="issue-body" className="issue-form-label">
            Description (Markdown)
          </label>
          <textarea
            id="issue-body"
            className="issue-form-textarea"
            placeholder="Describe the issue in detail..."
            value={body}
            onChange={(e) => setBody(e.target.value)}
            rows={8}
          />
        </div>

        {/* Labels */}
        <div className="issue-form-group">
          <label className="issue-form-label">Labels</label>
          <div className="issue-labels-selected">
            {selectedLabels.map((id) => {
              const label = labelsData?.labels.find((l) => l.id === id);
              if (!label) return null;
              return (
                <IssueLabelBadge
                  key={id}
                  label={label}
                  onRemove={() => toggleLabel(id)}
                />
              );
            })}
            <button
              type="button"
              className="issue-label-add-btn"
              onClick={() => setShowLabelPicker(!showLabelPicker)}
            >
              + Add Label
            </button>
          </div>

          {showLabelPicker && labelsData && (
            <div className="issue-label-picker">
              {labelsData.labels.length === 0 && (
                <span className="issue-label-picker-empty">
                  No labels yet. Create one in the repo settings.
                </span>
              )}
              {labelsData.labels.map((label) => (
                <button
                  key={label.id}
                  type="button"
                  className={`issue-label-picker-item ${
                    selectedLabels.includes(label.id) ? "selected" : ""
                  }`}
                  onClick={() => toggleLabel(label.id)}
                >
                  <span
                    className="issue-label-dot"
                    style={{ backgroundColor: label.color }}
                  />
                  {label.name}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Error */}
        {error && <div className="issue-form-error">⚠️ {error}</div>}

        {/* Actions */}
        <div className="issue-form-actions">
          <button
            type="button"
            className="issue-form-cancel"
            onClick={() => router.back()}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="issue-form-submit"
            disabled={submitting || !title.trim()}
          >
            {submitting ? "Creating..." : "🎯 Submit Issue"}
          </button>
        </div>
      </form>
    </div>
  );
}
