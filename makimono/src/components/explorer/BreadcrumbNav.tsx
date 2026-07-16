"use client";

import React from "react";
import Link from "next/link";
import { ChevronRight } from "lucide-react";

interface Crumb {
  label: string;
  href: string;
}

interface BreadcrumbNavProps {
  crumbs: Crumb[];
  repoName?: string; // affiché comme label racine si fourni
}

export default function BreadcrumbNav({ crumbs, repoName }: BreadcrumbNavProps) {
  // Si repoName fourni, on remplace le label du crumb repo (index 1) par repoName
  const displayCrumbs = crumbs.map((c, i) =>
    repoName && i === 1 ? { ...c, label: repoName } : c
  );

  return (
    <nav className="bc-nav" aria-label="Chemin du fichier">
      <ol className="bc-list">
        {displayCrumbs.map((crumb, i) => {
          const isLast = i === displayCrumbs.length - 1;
          // On skip le premier crumb (owner) et le deuxième (repo) car
          // ils sont déjà affichés dans le repo header
          if (i < 2) return null;

          return (
            <li key={crumb.href} className="bc-item">
              {isLast ? (
                <span className="bc-current" aria-current="page">
                  {crumb.label}
                </span>
              ) : (
                <>
                  <Link href={crumb.href} className="bc-link">
                    {crumb.label}
                  </Link>
                  <ChevronRight size={12} className="bc-sep" aria-hidden="true" />
                </>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
