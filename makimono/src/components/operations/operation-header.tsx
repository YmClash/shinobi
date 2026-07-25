"use client";

import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { Operation } from "@/lib/api";

interface OperationHeaderProps {
  operation: Operation;
  owner: string;
  repo: string;
}

function truncateId(id: string, len = 12): string {
  return id.length > len ? id.slice(0, len) + "…" : id;
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleString("fr-FR", {
    dateStyle: "long",
    timeStyle: "medium",
  });
}

export function OperationHeader({ operation, owner, repo }: OperationHeaderProps) {
  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
  };

  return (
    <div className="space-y-3">
      {/* Back + Title */}
      <div className="flex items-start gap-3">
        <Link href={`/${owner}/${repo}/operations`}>
          <Button variant="ghost" size="sm" className="gap-1 text-xs">
            ← Retour
          </Button>
        </Link>
        <div className="flex-1 min-w-0">
          <h1 className="text-lg font-bold tracking-wide truncate">
            {operation.description}
          </h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            {formatDate(operation.created_at)}
          </p>
        </div>
      </div>

      {/* Metadata badges */}
      <div className="flex flex-wrap gap-2 items-center">
        <Badge
          variant="outline"
          className="font-mono text-[10px] px-2 cursor-pointer hover:bg-accent/50 transition-colors"
          onClick={() => copyToClipboard(operation.id)}
          title="Cliquer pour copier l'UUID"
        >
          🆔 {truncateId(operation.id, 16)}
        </Badge>

        <Badge
          variant="outline"
          className="font-mono text-[10px] px-2 cursor-pointer hover:bg-accent/50 transition-colors"
          onClick={() => copyToClipboard(operation.content_id)}
          title="Cliquer pour copier le CID Jujutsu"
        >
          🔗 jj:{truncateId(operation.content_id, 16)}
        </Badge>

        {operation.ipfs_content_id && (
          <Badge
            variant="secondary"
            className="font-mono text-[10px] px-2 cursor-pointer hover:bg-accent/50 transition-colors"
            onClick={() => copyToClipboard(operation.ipfs_content_id!)}
            title="Cliquer pour copier le CID IPFS"
          >
            🌐 ipfs:{truncateId(operation.ipfs_content_id, 16)}
          </Badge>
        )}

        <Badge variant="outline" className="text-[10px] px-2">
          👤 {truncateId(operation.author_id, 8)}
        </Badge>

        {operation.parent_ids.length > 0 && (
          <Badge variant="outline" className="text-[10px] px-2">
            ⬆️ {operation.parent_ids.length} parent{operation.parent_ids.length > 1 ? "s" : ""}
          </Badge>
        )}
      </div>
    </div>
  );
}
