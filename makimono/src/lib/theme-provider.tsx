"use client";

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Theme = "ninja" | "cyberpunk" | "glass" | "scroll";

export const THEMES: { id: Theme; label: string; icon: string; description: string }[] = [
  { id: "ninja", label: "Ninja", icon: "🥷", description: "Noir profond, accents rouge sang" },
  { id: "cyberpunk", label: "Cyberpunk", icon: "⚡", description: "Néons violet/cyan sur fond sombre" },
  { id: "glass", label: "Glassmorphism", icon: "💎", description: "Surfaces floues semi-transparentes" },
  { id: "scroll", label: "Parchemin", icon: "📜", description: "Tons sépia, texture papier ancien" },
];

const STORAGE_KEY = "shinobi-theme";

interface ThemeContextType {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>("ninja");
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    // Read saved theme from localStorage
    const saved = localStorage.getItem(STORAGE_KEY) as Theme | null;
    if (saved && THEMES.some((t) => t.id === saved)) {
      setThemeState(saved);
      document.documentElement.setAttribute("data-theme", saved);
    } else {
      document.documentElement.setAttribute("data-theme", "ninja");
    }
    setMounted(true);
  }, []);

  const setTheme = (newTheme: Theme) => {
    setThemeState(newTheme);
    localStorage.setItem(STORAGE_KEY, newTheme);
    document.documentElement.setAttribute("data-theme", newTheme);
  };

  // Avoid hydration mismatch by not rendering until mounted
  if (!mounted) {
    return <div style={{ visibility: "hidden" }}>{children}</div>;
  }

  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    // Safe fallback for SSR / static generation
    return { theme: "ninja" as Theme, setTheme: () => {} };
  }
  return context;
}
