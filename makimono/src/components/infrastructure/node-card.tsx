"use client";

import { Card, CardContent } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface NodeCardProps {
  name: string;
  icon: string;
  technology: string;
  port: number;
  status: "online" | "offline" | "unknown";
  className?: string;
}

export function NodeCard({ name, icon, technology, port, status, className = "" }: NodeCardProps) {
  const statusColor = {
    online: "bg-green-500",
    offline: "bg-red-500",
    unknown: "bg-yellow-500",
  }[status];

  const statusLabel = {
    online: "Opérationnel",
    offline: "Hors ligne",
    unknown: "Inconnu",
  }[status];

  return (
    <Card className={`relative overflow-hidden group hover:border-primary/30 transition-all duration-300 glass-card neon-glow ${className}`}>
      <CardContent className="p-4">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <span className="text-2xl">{icon}</span>
            <div>
              <h3 className="font-semibold text-sm">{name}</h3>
              <p className="text-xs text-muted-foreground">{technology}</p>
            </div>
          </div>
          <Tooltip>
            <TooltipTrigger>
              <span
                className={`inline-block w-2.5 h-2.5 rounded-full ${statusColor} ${status === "online" ? "animate-pulse-glow" : ""}`}
                style={{ color: status === "online" ? "#22c55e" : status === "offline" ? "#ef4444" : "#eab308" }}
              />
            </TooltipTrigger>
            <TooltipContent side="top">{statusLabel}</TooltipContent>
          </Tooltip>
        </div>
        <div className="mt-3 flex items-center justify-between text-xs text-muted-foreground">
          <span className="font-mono">:{port}</span>
          <span className={status === "online" ? "text-green-400" : "text-red-400"}>
            {statusLabel}
          </span>
        </div>
      </CardContent>
      {/* Subtle gradient overlay on hover */}
      <div className="absolute inset-0 bg-gradient-to-br from-primary/0 to-primary/5 opacity-0 group-hover:opacity-100 transition-opacity duration-300 pointer-events-none" />
    </Card>
  );
}
