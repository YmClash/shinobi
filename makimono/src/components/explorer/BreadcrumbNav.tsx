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
}

export default function BreadcrumbNav({ crumbs }: BreadcrumbNavProps) {
  return (
    <nav className="breadcrumb-nav" aria-label="Chemin du fichier">
      <ol className="breadcrumb-list">
        {crumbs.map((crumb, i) => {
          const isLast = i === crumbs.length - 1;
          return (
            <li key={crumb.href} className="breadcrumb-item">
              {isLast ? (
                <span className="crumb-current" aria-current="page">
                  {crumb.label}
                </span>
              ) : (
                <>
                  <Link href={crumb.href} className="crumb-link">
                    {crumb.label}
                  </Link>
                  <ChevronRight
                    size={12}
                    className="crumb-sep"
                    aria-hidden="true"
                  />
                </>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
