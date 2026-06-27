"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState } from "react";
import { ThemeSwitcher } from "@/components/theme-switcher";
import { Separator } from "@/components/ui/separator";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useHealth } from "@/hooks/use-api";

const NAV_ITEMS = [
  { href: "/", icon: "⚙️", label: "Dashboard" },
  { href: "/search", icon: "🔍", label: "Recherche RAG" },
  { href: "/operations", icon: "📜", label: "Opérations" },
  { href: "/operations/new", icon: "⚔️", label: "Forger" },
];

export function Sidebar() {
  const pathname = usePathname();
  const [collapsed, setCollapsed] = useState(false);
  const { data: health } = useHealth();

  const isOnline = health?.status === "operational";

  return (
    <aside
      className={`
        flex flex-col h-full border-r border-border bg-sidebar text-sidebar-foreground
        transition-all duration-300 ease-in-out
        ${collapsed ? "w-16" : "w-56"}
      `}
    >
      {/* ── Logo / Brand ──────────────────────────── */}
      <div className="flex items-center gap-2 px-3 py-4">
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="text-xl hover:opacity-80 transition-opacity cursor-pointer flex-shrink-0"
          title={collapsed ? "Expand" : "Collapse"}
        >
          🥷
        </button>
        {!collapsed && (
          <div className="flex flex-col min-w-0">
            <span className="font-bold text-sm tracking-wider text-sidebar-primary">
              SHINOBI
            </span>
            <span className="text-[10px] text-muted-foreground tracking-wide">
              MAKIMONO
            </span>
          </div>
        )}
      </div>

      <Separator className="bg-sidebar-border" />

      {/* ── Navigation ────────────────────────────── */}
      <nav className="flex-1 flex flex-col gap-1 px-2 py-3">
        {NAV_ITEMS.map((item) => {
          const isActive = pathname === item.href;
          const linkBtn = (
            <Link href={item.href}>
              <Button
                variant={isActive ? "secondary" : "ghost"}
                size="sm"
                className={`
                  w-full gap-2 text-xs
                  ${collapsed ? "justify-center px-0" : "justify-start px-2"}
                  ${isActive ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium" : "text-sidebar-foreground hover:bg-sidebar-accent/50"}
                `}
              >
                <span className="text-base flex-shrink-0">{item.icon}</span>
                {!collapsed && <span className="truncate">{item.label}</span>}
              </Button>
            </Link>
          );

          if (collapsed) {
            return (
              <Tooltip key={item.href}>
                <TooltipTrigger render={<div />}>
                  {linkBtn}
                </TooltipTrigger>
                <TooltipContent side="right">{item.label}</TooltipContent>
              </Tooltip>
            );
          }

          return <div key={item.href}>{linkBtn}</div>;
        })}
      </nav>

      {/* ── Bottom section ────────────────────────── */}
      <div className="mt-auto px-2 pb-3 space-y-2">
        <Separator className="bg-sidebar-border" />

        {/* System status */}
        <div className={`flex items-center gap-2 px-2 py-1 ${collapsed ? "justify-center" : ""}`}>
          <span
            className={`inline-block w-2 h-2 rounded-full flex-shrink-0 ${
              isOnline ? "bg-green-500 animate-pulse-glow" : "bg-red-500"
            }`}
            style={{ color: isOnline ? "#22c55e" : "#ef4444" }}
          />
          {!collapsed && (
            <span className="text-[10px] text-muted-foreground">
              {isOnline ? "Taijutsu Online" : "Offline"}
            </span>
          )}
        </div>

        {/* Theme switcher */}
        <ThemeSwitcher />
      </div>
    </aside>
  );
}
