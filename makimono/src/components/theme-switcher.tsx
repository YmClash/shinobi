"use client";

import { useTheme, THEMES, type Theme } from "@/lib/theme-provider";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function ThemeSwitcher() {
  const { theme, setTheme } = useTheme();
  const current = THEMES.find((t) => t.id === theme)!;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        id="theme-switcher"
        className="flex items-center gap-2 text-xs w-full justify-start px-2 py-1.5 rounded-md hover:bg-accent/50 transition-colors cursor-pointer"
      >
        <span className="text-base">{current.icon}</span>
        <span className="truncate text-sm">{current.label}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        {THEMES.map((t) => (
          <DropdownMenuItem
            key={t.id}
            onClick={() => setTheme(t.id as Theme)}
            className={`gap-3 cursor-pointer ${theme === t.id ? "bg-accent" : ""}`}
          >
            <span className="text-lg">{t.icon}</span>
            <div className="flex flex-col">
              <span className="font-medium text-sm">{t.label}</span>
              <span className="text-xs text-muted-foreground">{t.description}</span>
            </div>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
