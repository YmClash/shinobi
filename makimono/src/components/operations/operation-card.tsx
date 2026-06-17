"use client";

import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Operation } from "@/lib/api";

interface OperationCardProps {
  operation: Operation;
  className?: string;
}

function timeAgo(dateStr: string): string {
  const now = Date.now();
  const date = new Date(dateStr).getTime();
  const diff = now - date;

  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (days > 0) return `il y a ${days}j`;
  if (hours > 0) return `il y a ${hours}h`;
  if (minutes > 0) return `il y a ${minutes}min`;
  return "à l'instant";
}

function truncateId(id: string, len = 8): string {
  return id.length > len ? id.slice(0, len) + "…" : id;
}

export function OperationCard({ operation, className = "" }: OperationCardProps) {
  return (
    <Card className={`glass-card neon-glow hover:border-primary/20 transition-all duration-200 ${className}`}>
      <CardContent className="p-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium truncate">{operation.description}</p>
            <div className="flex items-center gap-2 mt-1.5 text-xs text-muted-foreground">
              <Tooltip>
                <TooltipTrigger>
                  <span className="font-mono">{truncateId(operation.author_id)}</span>
                </TooltipTrigger>
                <TooltipContent side="top">{operation.author_id}</TooltipContent>
              </Tooltip>
              <span>·</span>
              <span>{timeAgo(operation.created_at)}</span>
            </div>
          </div>
        </div>

        {/* CIDs */}
        <div className="flex items-center gap-1.5 mt-2 flex-wrap">
          <Tooltip>
            <TooltipTrigger>
              <Badge variant="outline" className="text-[10px] font-mono px-1.5 py-0">
                jj:{truncateId(operation.content_id, 10)}
              </Badge>
            </TooltipTrigger>
            <TooltipContent side="top" className="font-mono text-xs max-w-xs break-all">
              {operation.content_id}
            </TooltipContent>
          </Tooltip>

          {operation.ipfs_content_id && (
            <Tooltip>
              <TooltipTrigger>
                <Badge variant="secondary" className="text-[10px] font-mono px-1.5 py-0">
                  ipfs:{truncateId(operation.ipfs_content_id, 10)}
                </Badge>
              </TooltipTrigger>
              <TooltipContent side="top" className="font-mono text-xs max-w-xs break-all">
                {operation.ipfs_content_id}
              </TooltipContent>
            </Tooltip>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
