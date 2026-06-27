"use client";

import { Badge } from "@/components/ui/badge";

interface SimilarityBadgeProps {
  score: number;
  className?: string;
}

export function SimilarityBadge({ score, className = "" }: SimilarityBadgeProps) {
  const percentage = Math.round(score * 100);

  let colorClass: string;
  if (score >= 0.6) {
    colorClass = "bg-green-500/15 text-green-400 border-green-500/30";
  } else if (score >= 0.4) {
    colorClass = "bg-yellow-500/15 text-yellow-400 border-yellow-500/30";
  } else if (score >= 0.3) {
    colorClass = "bg-orange-500/15 text-orange-400 border-orange-500/30";
  } else {
    colorClass = "bg-red-500/15 text-red-400 border-red-500/30";
  }

  return (
    <Badge
      variant="outline"
      className={`font-mono text-[11px] px-1.5 py-0 ${colorClass} ${className}`}
    >
      {percentage}%
    </Badge>
  );
}
