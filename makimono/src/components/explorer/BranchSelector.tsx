"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { GitBranch, Tag, ChevronDown, Check } from "lucide-react";
import type { RefItem } from "@/lib/explorer-api";
import { buildTreeUrl } from "@/lib/explorer-api";

interface BranchSelectorProps {
  owner: string;
  repo: string;
  currentRevision: string;
  currentPath: string;
  branches: RefItem[];
  tags: RefItem[];
}

export default function BranchSelector({
  owner,
  repo,
  currentRevision,
  currentPath,
  branches,
  tags,
}: BranchSelectorProps) {
  const [open, setOpen] = useState(false);
  const router = useRouter();

  const handleSelect = (name: string) => {
    setOpen(false);
    const url = buildTreeUrl(owner, repo, name, currentPath);
    router.push(url);
  };

  return (
    <div className="branch-selector">
      <button
        className="branch-trigger"
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <GitBranch size={14} />
        <span className="branch-current">{currentRevision}</span>
        <ChevronDown
          size={14}
          className={`branch-chevron ${open ? "chevron-open" : ""}`}
        />
      </button>

      {open && (
        <>
          {/* Overlay pour fermer */}
          <div
            className="branch-overlay"
            onClick={() => setOpen(false)}
            aria-hidden="true"
          />
          <div className="branch-dropdown" role="listbox">
            {branches.length > 0 && (
              <div className="branch-group">
                <div className="branch-group-label">
                  <GitBranch size={12} />
                  Branches
                </div>
                {branches.map((b) => (
                  <button
                    key={b.name}
                    className="branch-option"
                    role="option"
                    aria-selected={b.name === currentRevision}
                    onClick={() => handleSelect(b.name)}
                  >
                    {b.name === currentRevision && (
                      <Check size={12} className="option-check" />
                    )}
                    <span className="option-name">{b.name}</span>
                    <span className="option-sha">
                      {b.target.slice(0, 7)}
                    </span>
                  </button>
                ))}
              </div>
            )}

            {tags.length > 0 && (
              <div className="branch-group">
                <div className="branch-group-label">
                  <Tag size={12} />
                  Tags
                </div>
                {tags.map((t) => (
                  <button
                    key={t.name}
                    className="branch-option"
                    role="option"
                    aria-selected={t.name === currentRevision}
                    onClick={() => handleSelect(t.name)}
                  >
                    {t.name === currentRevision && (
                      <Check size={12} className="option-check" />
                    )}
                    <span className="option-name">{t.name}</span>
                    <span className="option-sha">
                      {t.target.slice(0, 7)}
                    </span>
                  </button>
                ))}
              </div>
            )}

            {branches.length === 0 && tags.length === 0 && (
              <div className="branch-empty">Aucune référence trouvée</div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
