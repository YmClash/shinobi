"use client";

// ═══════════════════════════════════════════════════════════════
// MentionRenderer — Phase 37C (Le Mégaphone) + P1 Fix
//
// Transforme les @mentions dans un texte en liens cliquables :
// - @handle local  → Link violet vers /{handle}
// - @user@domain   → Lien externe vers https://domain/@user
//
// P1 Fix : Le backend fournit un tableau `validatedMentions` de
// handles confirmés par l'AST Markdown. Seuls ces handles sont
// linkifiés. Zéro faux positif (`@babel` reste du texte brut).
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
  /**
   * Liste des handles validés par le backend (AST-aware).
   * Seuls les handles présents dans cette liste seront linkifiés.
   * Inclut les handles locaux ("yusuf") et fédérés ("alice@mastodon.social").
   * Si absent, aucune mention n'est linkifiée (sécurité par défaut).
   */
  validatedMentions?: string[];
}

// Regex JS côté client — miroir du parseur Rust
// Capture @handle ou @user@domain.tld
const MENTION_RE = /(?:^|[\s(\[{])(@([a-zA-Z0-9_-]+(?:@[a-zA-Z0-9][a-zA-Z0-9._-]+)?))/g;

/**
 * Rend un texte avec les @mentions transformées en liens cliquables.
 *
 * - Mentions locales (`@yusuf`) → lien violet vers le profil `/yusuf`
 * - Mentions fédérées (`@alice@mastodon.social`) → lien externe bleu
 *
 * P1 Fix: Si `validatedMentions` est fourni, seuls les handles
 * validés par le backend sont linkifiés. Les faux positifs comme
 * `@babel` dans du code inline restent du texte brut.
 */
export function MentionRenderer({ text, className, validatedMentions }: MentionRendererProps) {
  if (!text) return null;

  // Créer un Set pour une lookup O(1)
  const validatedSet = validatedMentions
    ? new Set(validatedMentions)
    : null;

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

    // P1 Fix: Si le backend a fourni une liste de mentions validées,
    // ne linkifier QUE les handles confirmés.
    if (validatedSet && !validatedSet.has(rawHandle)) {
      // Handle non validé → laisser en texte brut, continuer
      continue;
    }

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
