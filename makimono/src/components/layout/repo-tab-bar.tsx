"use client";

// ═══════════════════════════════════════════════════════════════
// Repo Tab Bar — GitHub-style horizontal navigation (Phase 26B)
// Appears at the top of all /[owner]/[repo]/* pages
// ═══════════════════════════════════════════════════════════════

import Link from "next/link";
import { usePathname } from "next/navigation";

interface RepoTabBarProps {
  owner: string;
  repo: string;
}

const tabs = [
  { key: "code",        label: "Code",             icon: "📄", match: (p: string) => p.includes("/tree/") || /^\/[^/]+\/[^/]+$/.test(p) },
  { key: "commits",     label: "Commits",          icon: "📝", match: (p: string) => p.includes("/commits") },
  { key: "mrs",         label: "Merge Requests",   icon: "⚔️", match: (p: string) => p.includes("/mrs") },
  { key: "ops",         label: "Operations",       icon: "⚡", match: (p: string) => /\/operations(?!\/)/.test(p) || /\/operations\//.test(p) },
  { key: "refs",        label: "Bookmarks",        icon: "🔖", match: (p: string) => p.includes("/bookmarks") },
  { key: "checkpoints", label: "AI Checkpoints",   icon: "🧠", match: (p: string) => p.includes("/checkpoints") },
];

function buildHref(owner: string, repo: string, key: string): string {
  const base = `/${owner}/${repo}`;
  switch (key) {
    case "code":    return base;
    case "commits": return `${base}/commits`;
    case "mrs":     return `${base}/mrs`;
    case "ops":     return `${base}/operations`;
    case "refs":    return `${base}/bookmarks`;
    case "checkpoints": return `${base}/checkpoints`;
    default:        return base;
  }
}

export function RepoTabBar({ owner, repo }: RepoTabBarProps) {
  const pathname = usePathname();

  return (
    <nav className="repo-tab-bar">
      {tabs.map((tab) => {
        const href = buildHref(owner, repo, tab.key);
        const isActive = tab.match(pathname);

        return (
          <Link
            key={tab.key}
            href={href}
            className={`repo-tab ${isActive ? "repo-tab-active" : ""}`}
          >
            <span className="repo-tab-icon">{tab.icon}</span>
            <span className="repo-tab-label">{tab.label}</span>
          </Link>
        );
      })}
    </nav>
  );
}
