"use client";

// ═══════════════════════════════════════════════════════════════
// MentionRenderer — Phase 37C (Le Mégaphone)
//
// Transforme les @mentions dans un texte en liens cliquables :
// - @handle local  → Link violet vers /{handle}
// - @user@domain   → Lien externe vers https://domain/@user
//
// N'altère PAS le body stocké en BDD — transformation côté client.
// ═══════════════════════════════════════════════════════════════

import React from "react";
import Link from "next/link";

interface MentionRendererProps {
  /** Texte brut contenant potentiellement des @mentions. */
  text: string;
  /** Classe CSS additionnelle pour le wrapper. */
  className?: string;
}

// Regex JS côté client — miroir du parseur Rust
// Capture @handle ou @user@domain.tld
const MENTION_RE = /(?:^|[\s(\[{])(@([a-zA-Z0-9_-]+(?:@[a-zA-Z0-9][a-zA-Z0-9._-]+)?))/g;

/**
 * Rend un texte avec les @mentions transformées en liens cliquables.
 *
 * - Mentions locales (`@yusuf`) → lien violet vers le profil `/yusuf`
 * - Mentions fédérées (`@alice@mastodon.social`) → lien externe bleu
 */
export function MentionRenderer({ text, className }: MentionRendererProps) {
  if (!text) return null;

  const parts: React.ReactNode[] = [];
  let lastIndex = 0;

  // Reset regex state
  MENTION_RE.lastIndex = 0;

  let match: RegExpExecArray | null;
  while ((match = MENTION_RE.exec(text)) !== null) {
    const fullMatch = match[0];
    const mentionWithAt = match[1]; // "@handle" ou "@user@domain"
    const rawHandle = match[2]; // "handle" ou "user@domain"

    // Index du @ dans le match (skip le whitespace/punc avant)
    const atIndex = fullMatch.indexOf("@");
    const mentionStart = match.index + atIndex;

    // Texte avant la mention
    if (mentionStart > lastIndex) {
      parts.push(text.slice(lastIndex, mentionStart));
    }

    // Déterminer si c'est une mention fédérée
    const atPos = rawHandle.indexOf("@");
    if (atPos > 0) {
      // Mention fédérée → lien externe
      const localPart = rawHandle.slice(0, atPos);
      const domain = rawHandle.slice(atPos + 1);
      parts.push(
        <a
          key={`mention-${mentionStart}`}
          href={`https://${domain}/@${localPart}`}
          target="_blank"
          rel="noopener noreferrer"
          className="mention-link mention-link-remote"
          title={`Profil fédéré: ${rawHandle}`}
        >
          @{rawHandle}
        </a>
      );
    } else {
      // Mention locale → Link interne
      parts.push(
        <Link
          key={`mention-${mentionStart}`}
          href={`/${rawHandle}`}
          className="mention-link mention-link-local"
          title={`Profil de ${rawHandle}`}
        >
          @{rawHandle}
        </Link>
      );
    }

    lastIndex = mentionStart + mentionWithAt.length;
  }

  // Texte restant après la dernière mention
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }

  // Pas de mentions → retourner le texte brut
  if (parts.length === 0) {
    return <span className={className}>{text}</span>;
  }

  return <span className={className}>{parts}</span>;
}
