"use client";

import { useState } from "react";
import { useParams } from "next/navigation";
import { Skeleton } from "@/components/ui/skeleton";
import { useOperation, useOperationChunks, useOperationDiff, useCommitDiff } from "@/hooks/use-api";
import { buildRepoPrefix } from "@/lib/api";
import { OperationHeader } from "@/components/operations/operation-header";
import { FileExplorer } from "@/components/operations/file-explorer";
import { IpfsExplorer } from "@/components/operations/ipfs-explorer";
import { OracleReview } from "@/components/operations/oracle-review";
import { UnifiedDiffViewer } from "@/components/operations/unified-diff-viewer";

type Tab = "chunks" | "diff" | "ipfs";

export default function OperationDetailPage() {
  const params = useParams();
  const owner = params.owner as string;
  const repo = params.repo as string;
  const id = params.id as string;
  const repoPrefix = buildRepoPrefix(owner, repo);

  const [activeTab, setActiveTab] = useState<Tab>("chunks");

  const { data: operation, loading: loadingOp, error: errorOp } = useOperation(repoPrefix, id);
  const { data: chunksData, loading: loadingChunks } = useOperationChunks(repoPrefix, id);
  const { data: diffData, loading: loadingDiff } = useOperationDiff(repoPrefix, id);
  const { data: diffContent, loading: loadingDiffContent } = useCommitDiff(repoPrefix, id);

  if (errorOp) {
    return (
      <div className="max-w-4xl mx-auto">
        <div className="text-sm text-destructive bg-destructive/10 rounded-lg p-4">
          Erreur : {errorOp}
        </div>
      </div>
    );
  }

  if (loadingOp || !operation) {
    return (
      <div className="max-w-4xl mx-auto space-y-4">
        <Skeleton className="h-20 w-full rounded-lg" />
        <Skeleton className="h-10 w-64 rounded-lg" />
        <Skeleton className="h-[400px] w-full rounded-lg" />
      </div>
    );
  }

  const tabs: { id: Tab; label: string; icon: string; count?: number }[] = [
    {
      id: "chunks",
      label: "Fichiers & Chunks",
      icon: "🧩",
      count: chunksData?.count,
    },
    {
      id: "diff",
      label: "Diff Complet",
      icon: "📝",
      count: diffContent?.stats.files_changed ?? diffData?.count,
    },
    {
      id: "ipfs" as Tab,
      label: "IPFS Explorer",
      icon: "🌐",
    },
  ];

  return (
    <div className="max-w-5xl mx-auto space-y-6">
      {/* ── Header ──────────────────────────── */}
      <OperationHeader operation={operation} />

      {/* ── Oracle Review (auto-revealed) ──── */}
      <OracleReview operationId={id} repoPrefix={repoPrefix} />

      {/* ── Tabs ────────────────────────────── */}
      <div className="flex gap-1 border-b border-border/50 pb-px">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`
              flex items-center gap-1.5 px-3 py-2 text-xs font-medium rounded-t-md
              transition-colors cursor-pointer
              ${activeTab === tab.id
                ? "bg-accent text-accent-foreground border-b-2 border-primary"
                : "text-muted-foreground hover:text-foreground hover:bg-accent/30"
              }
            `}
          >
            <span>{tab.icon}</span>
            <span>{tab.label}</span>
            {tab.count !== undefined && (
              <span className="text-[10px] bg-muted px-1.5 py-0.5 rounded-full ml-1">
                {tab.count}
              </span>
            )}
          </button>
        ))}
      </div>

      {/* ── Tab Content ─────────────────────── */}
      <div className="min-h-[300px]">
        {activeTab === "chunks" && (
          loadingChunks ? (
            <div className="space-y-3">
              {[...Array(3)].map((_, i) => (
                <Skeleton key={i} className="h-32 w-full rounded-lg" />
              ))}
            </div>
          ) : (
            <FileExplorer chunks={chunksData?.chunks ?? []} />
          )
        )}

        {activeTab === "diff" && (
          loadingDiffContent ? (
            <div className="commits-loading">
              <div className="commits-spinner" />
              <span>Chargement du diff colorisé...</span>
            </div>
          ) : diffContent ? (
            <UnifiedDiffViewer
              files={diffContent.files}
              stats={diffContent.stats}
            />
          ) : (
            /* Fallback : ancien diff (liste de fichiers) si l'API diff-content échoue */
            <div className="diff-files">
              <div className="diff-file-tree">
                {(diffData?.changed_files ?? []).map((file) => (
                  <div key={file} className="diff-file-tree-item">
                    <span className="diff-file-status diff-file-status-added">A</span>
                    <span className="diff-file-tree-name">{file}</span>
                  </div>
                ))}
              </div>
              {(diffData?.changed_files ?? []).length === 0 && (
                <div className="commits-empty">
                  <p>Aucun fichier modifié détecté</p>
                </div>
              )}
            </div>
          )
        )}

        {activeTab === "ipfs" && (
          <IpfsExplorer operationId={id} repoPrefix={repoPrefix} />
        )}
      </div>
    </div>
  );
}
