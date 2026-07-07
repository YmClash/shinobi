"use client";

import React from "react";
import { useRouter } from "next/navigation";
import { Folder, FileCode, File, FileText } from "lucide-react";
import type { TreeEntryItem } from "@/lib/explorer-api";
import { buildTreeUrl } from "@/lib/explorer-api";

interface FileBrowserProps {
  owner: string;
  repo: string;
  revision: string;
  entries: TreeEntryItem[];
}

/** Choisit l'icône en fonction du nom/type de fichier */
function EntryIcon({ entry }: { entry: TreeEntryItem }) {
  if (entry.kind === "directory") {
    return <Folder size={16} className="explorer-icon icon-dir" />;
  }
  const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
  if (["ts", "tsx", "js", "jsx", "rs", "py", "go"].includes(ext)) {
    return <FileCode size={16} className="explorer-icon icon-code" />;
  }
  if (["md", "txt", "json", "toml", "yaml", "yml"].includes(ext)) {
    return <FileText size={16} className="explorer-icon icon-text" />;
  }
  return <File size={16} className="explorer-icon icon-file" />;
}

/** Formatte une taille en octets de façon lisible */
function formatSize(bytes: number | null): string {
  if (bytes === null) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function FileBrowser({
  owner,
  repo,
  revision,
  entries,
}: FileBrowserProps) {
  const router = useRouter();

  const handleNavigate = (entry: TreeEntryItem) => {
    const url = buildTreeUrl(owner, repo, revision, entry.path);
    router.push(url);
  };

  if (entries.length === 0) {
    return (
      <div className="explorer-empty">
        <Folder size={48} className="explorer-empty-icon" />
        <p>Ce répertoire est vide</p>
      </div>
    );
  }

  return (
    <div className="file-browser">
      <table className="browser-table">
        <thead>
          <tr>
            <th className="col-name">Nom</th>
            <th className="col-size">Taille</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((entry) => (
            <tr
              key={entry.path}
              className="browser-row"
              onClick={() => handleNavigate(entry)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") handleNavigate(entry);
              }}
            >
              <td className="col-name">
                <span className="entry-icon-wrap">
                  <EntryIcon entry={entry} />
                </span>
                <span
                  className={`entry-name ${
                    entry.kind === "directory" ? "name-dir" : "name-file"
                  }`}
                >
                  {entry.name}
                </span>
              </td>
              <td className="col-size">
                {entry.kind === "file" && (
                  <span className="entry-size">{formatSize(entry.size)}</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
