"use client";

import { useState } from "react";

interface CopyButtonProps {
  text: string;
  className?: string;
}

/**
 * Tiny client-side copy-to-clipboard button.
 * Isolated as "use client" so the parent Server Component (ShikiCodeBlock)
 * can stay server-only (zero JS for the highlighting itself).
 */
export function CopyButton({ text, className = "" }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback — ignore
    }
  };

  return (
    <button
      onClick={handleCopy}
      className={`
        h-6 px-2 text-[10px] font-medium rounded
        text-muted-foreground hover:text-foreground
        hover:bg-accent/50 transition-colors cursor-pointer
        ${className}
      `}
    >
      {copied ? "✓ Copié" : "Copier"}
    </button>
  );
}
