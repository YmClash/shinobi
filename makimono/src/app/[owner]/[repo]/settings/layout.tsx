"use client";

// ═══════════════════════════════════════════════════════════════
// Settings Layout — sous-navigation (Phase 42-pre-Fix)
// /[owner]/[repo]/settings/{general,webhooks}
// ═══════════════════════════════════════════════════════════════

import Link from "next/link";
import { useParams, usePathname } from "next/navigation";

const SECTIONS = [
  { key: "general", label: "General", icon: "🔒" },
  { key: "webhooks", label: "Webhooks", icon: "🔔" },
];

export default function SettingsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const params = useParams<{ owner: string; repo: string }>();
  const pathname = usePathname();
  const base = `/${params.owner}/${params.repo}/settings`;

  return (
    <div className="st-shell">
      <nav className="st-subnav" aria-label="Repository settings">
        {SECTIONS.map((s) => {
          const active = pathname.startsWith(`${base}/${s.key}`);
          return (
            <Link
              key={s.key}
              id={`settings-nav-${s.key}`}
              href={`${base}/${s.key}`}
              className={`st-subnav-item ${active ? "st-subnav-active" : ""}`}
            >
              <span aria-hidden>{s.icon}</span>
              {s.label}
            </Link>
          );
        })}
      </nav>
      <div className="st-content">{children}</div>
    </div>
  );
}
