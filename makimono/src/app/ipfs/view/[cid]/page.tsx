"use client";

import { useState, useEffect } from "react";
import { useParams } from "next/navigation";
import {
  type Manifest,
  type ManifestFile,
  FileRenderer,
  fileIcon,
  formatSize,
  isImageFile,
} from "@/components/viewers/artifact-viewers";

// ── Page Component ───────────────────────────────────────────────────────

export default function IpfsViewerPage() {
  const params = useParams<{ cid: string }>();
  const cid = params.cid;

  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [selectedFile, setSelectedFile] = useState<ManifestFile | null>(null);
  const [fileContent, setFileContent] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [fileLoading, setFileLoading] = useState(false);
  const [error, setError] = useState<string>("");

  // Fetch the root manifest
  useEffect(() => {
    if (!cid) return;
    setLoading(true);
    fetch(`/api/ipfs/${cid}`)
      .then((r) => {
        if (!r.ok) throw new Error(`IPFS gateway returned ${r.status}`);
        return r.json();
      })
      .then((data: Manifest) => {
        setManifest(data);
        setLoading(false);
      })
      .catch((e) => {
        setError(e.message);
        setLoading(false);
      });
  }, [cid]);

  // Fetch individual file content
  function openFile(file: ManifestFile) {
    setSelectedFile(file);
    setFileContent("");

    // Images don't need text fetch — render directly via <img src>
    if (isImageFile(file.path)) {
      setFileLoading(false);
      return;
    }

    setFileLoading(true);
    fetch(`/api/ipfs/${file.cid}`)
      .then((r) => r.text())
      .then((text) => {
        setFileContent(text);
        setFileLoading(false);
      })
      .catch(() => {
        setFileContent("Failed to load file from IPFS.");
        setFileLoading(false);
      });
  }

  if (loading) {
    return (
      <div className="ipfs-viewer-loading">
        <div className="ipfs-viewer-spinner" />
        <p>Loading IPFS content...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="ipfs-viewer-error">
        <span>⚠️</span>
        <p>{error}</p>
      </div>
    );
  }

  return (
    <div className="ipfs-viewer-page">
      {/* Header */}
      <div className="ipfs-viewer-header">
        <div className="ipfs-viewer-header-left">
          <span className="ipfs-viewer-header-icon">📦</span>
          <div>
            <h1 className="ipfs-viewer-title">IPFS Artifact Viewer</h1>
            <p className="ipfs-viewer-subtitle">
              {manifest?.description || "Checkpoint artifacts"}
            </p>
          </div>
        </div>
        <code className="ipfs-viewer-cid">{cid}</code>
      </div>

      <div className="ipfs-viewer-layout">
        {/* Sidebar — File list */}
        <aside className="ipfs-viewer-sidebar">
          <div className="ipfs-viewer-sidebar-header">
            Files ({manifest?.files.length ?? 0})
          </div>
          {manifest?.files.map((f) => (
            <button
              key={f.cid}
              className={`ipfs-viewer-file-btn ${selectedFile?.cid === f.cid ? "active" : ""}`}
              onClick={() => openFile(f)}
            >
              <span className="ipfs-viewer-file-icon">{fileIcon(f.path)}</span>
              <div className="ipfs-viewer-file-info">
                <span className="ipfs-viewer-file-name">{f.path}</span>
                <span className="ipfs-viewer-file-size">
                  {formatSize(f.size)}
                </span>
              </div>
            </button>
          ))}
        </aside>

        {/* Content area */}
        <main className="ipfs-viewer-content">
          {!selectedFile ? (
            <div className="ipfs-viewer-placeholder">
              <span className="ipfs-viewer-placeholder-icon">👈</span>
              <p>Select a file to view its contents</p>
            </div>
          ) : fileLoading ? (
            <div className="ipfs-viewer-loading">
              <div className="ipfs-viewer-spinner" />
              <p>Loading {selectedFile.path}...</p>
            </div>
          ) : (
            <FileRenderer file={selectedFile} content={fileContent} />
          )}
        </main>
      </div>
    </div>
  );
}
