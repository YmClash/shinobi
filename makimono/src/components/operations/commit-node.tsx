"use client";

import { memo } from "react";
import { Handle, Position, type NodeProps } from "@xyflow/react";

export interface CommitNodeData {
  id: string;
  description: string;
  authorId: string;
  createdAt: string;
  contentId: string;
  ipfsCid?: string;
  isHead: boolean;
  [key: string]: unknown;
}

function truncateId(id: string, len = 8): string {
  return id.length > len ? id.slice(0, len) + "…" : id;
}

function timeAgo(dateStr: string): string {
  const now = Date.now();
  const date = new Date(dateStr).getTime();
  const diff = now - date;
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  if (days > 0) return `${days}j`;
  if (hours > 0) return `${hours}h`;
  if (minutes > 0) return `${minutes}min`;
  return "now";
}

function CommitNodeComponent({ data }: NodeProps) {
  const nodeData = data as CommitNodeData;

  return (
    <div
      className={`
        relative px-4 py-3 rounded-lg border min-w-[220px] max-w-[280px]
        transition-all duration-200
        glass-card neon-glow
        ${nodeData.isHead
          ? "border-primary/60 ring-1 ring-primary/30 shadow-lg shadow-primary/10"
          : "border-border/50 hover:border-primary/30"
        }
      `}
    >
      {/* Top handle for edges coming in */}
      <Handle
        type="target"
        position={Position.Top}
        className="!bg-primary !w-2 !h-2 !border-0"
      />

      {/* Head indicator */}
      {nodeData.isHead && (
        <div className="absolute -top-2 -right-2 bg-primary text-primary-foreground text-[9px] font-bold px-1.5 py-0.5 rounded-full tracking-wider">
          HEAD
        </div>
      )}

      {/* Description */}
      <p className="text-xs font-medium leading-tight truncate mb-2">
        {nodeData.description || "No description"}
      </p>

      {/* Metadata row */}
      <div className="flex items-center justify-between gap-2 text-[10px] text-muted-foreground">
        <span className="font-mono opacity-70">{truncateId(nodeData.id)}</span>
        <span className="opacity-50">{timeAgo(nodeData.createdAt)}</span>
      </div>

      {/* CID badges */}
      <div className="flex items-center gap-1 mt-1.5 flex-wrap">
        <span className="font-mono text-[9px] bg-muted/50 px-1 py-0.5 rounded text-muted-foreground">
          jj:{truncateId(nodeData.contentId, 10)}
        </span>
        {nodeData.ipfsCid && (
          <span className="font-mono text-[9px] bg-chart-2/10 text-chart-2 px-1 py-0.5 rounded">
            ipfs:{truncateId(nodeData.ipfsCid, 10)}
          </span>
        )}
      </div>

      {/* Bottom handle for edges going out */}
      <Handle
        type="source"
        position={Position.Bottom}
        className="!bg-muted-foreground !w-2 !h-2 !border-0"
      />
    </div>
  );
}

export const CommitNode = memo(CommitNodeComponent);
