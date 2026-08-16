//! Mention Parser — Phase 37C (Le Mégaphone).
//!
//! Extrait les `@mentions` d'un texte Markdown de manière robuste :
//! 1. Parse le Markdown via `pulldown-cmark` (AST complet)
//! 2. Itère uniquement sur les nœuds `Text` (hors `Code` et `CodeBlock`)
//! 3. Applique une regex sur le contenu textuel pur
//! 4. Déduplique les mentions
//!
//! ## Pourquoi un AST et pas une simple Regex ?
//! Regex seul échouerait sur :
//! - Blocs de code contenant `@handle` (``` SELECT * FROM users WHERE email = '@admin' ```)
//! - Code inline `` `@variable` ``
//! - HTML brut avec attribut `id="@test"`
//!
//! `pulldown-cmark` génère un AST CommonMark conforme, garantissant que seuls
//! les nœuds textuels (paragraphes, listes, citations) sont analysés.

use std::collections::HashSet;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::LazyLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Mention extraite d'un texte Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mention {
    /// Handle brut tel qu'écrit (sans le `@` initial).
    /// Ex: `"yusuf"` ou `"alice@mastodon.social"`.
    pub raw: String,
    /// Partie locale du handle (avant le `@domaine` si fédéré).
    pub local_handle: String,
    /// Domaine distant (`None` si mention locale).
    pub domain: Option<String>,
}

impl Mention {
    /// Cette mention cible-t-elle un acteur distant (fédéré) ?
    pub fn is_remote(&self) -> bool {
        self.domain.is_some()
    }
}

// ── Regex (compilée une seule fois via LazyLock) ────────────────────────

/// Pattern pour capturer `@handle` ou `@user@domain.tld`.
///
/// Règles :
/// - Doit être précédé par un début de ligne, un espace, ou un caractère
///   de ponctuation d'ouverture `(`, `[`, `{`.
/// - Le handle est composé de `[a-zA-Z0-9_-]` (1+ caractères).
/// - Optionnellement suivi de `@domaine.tld` pour les mentions fédérées.
/// - Le domaine est composé de `[a-zA-Z0-9._-]` (2+ caractères).
static MENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s(\[{])@([a-zA-Z0-9_-]+(?:@[a-zA-Z0-9][a-zA-Z0-9._-]+)?)").unwrap()
});

// ── API publique ────────────────────────────────────────────────────────

/// Extrait toutes les `@mentions` d'un texte Markdown.
///
/// Utilise `pulldown-cmark` pour ne scanner que le contenu textuel pur,
/// ignorant automatiquement les blocs de code, le code inline, et le HTML brut.
///
/// Les mentions sont **dédupliquées** : `@yusuf @yusuf @yusuf` → 1 mention.
///
/// ## Exemples
/// ```
/// use domain::entities::mention::extract_mentions;
///
/// let mentions = extract_mentions("Salut @yusuf, tu peux review ?");
/// assert_eq!(mentions.len(), 1);
/// assert_eq!(mentions[0].local_handle, "yusuf");
/// assert!(mentions[0].domain.is_none());
/// ```
pub fn extract_mentions(text: &str) -> Vec<Mention> {
    if text.is_empty() {
        return Vec::new();
    }

    let parser = Parser::new(text);
    let mut mentions = Vec::new();
    let mut seen = HashSet::new();
    let mut in_code = false;

    for event in parser {
        match event {
            // Entrer dans un bloc de code ou code inline → ignorer le contenu
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,

            // Le code inline est déjà un événement `Code` distinct
            Event::Code(_) => { /* ignoré — code inline */ }

            // HTML brut — ignoré (pourrait contenir des faux positifs)
            Event::Html(_) | Event::InlineHtml(_) => { /* ignoré */ }

            // Texte pur → appliquer la regex
            Event::Text(ref cow) if !in_code => {
                for cap in MENTION_RE.captures_iter(cow.as_ref()) {
                    if let Some(raw) = cap.get(1) {
                        let raw_str = raw.as_str().to_string();

                        // Déduplique par handle brut (case-sensitive)
                        if seen.contains(&raw_str) {
                            continue;
                        }
                        seen.insert(raw_str.clone());

                        // Séparer local_handle et domain
                        let (local_handle, domain) = if let Some(at_pos) = raw_str.find('@') {
                            let local = raw_str[..at_pos].to_string();
                            let dom = raw_str[at_pos + 1..].to_string();
                            (local, Some(dom))
                        } else {
                            (raw_str.clone(), None)
                        };

                        mentions.push(Mention {
                            raw: raw_str,
                            local_handle,
                            domain,
                        });
                    }
                }
            }

            _ => {}
        }
    }

    mentions
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_local_mention() {
        let mentions = extract_mentions("Salut @yusuf, tu peux review ?");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "yusuf");
        assert!(mentions[0].domain.is_none());
        assert!(!mentions[0].is_remote());
    }

    #[test]
    fn test_multiple_local_mentions() {
        let mentions = extract_mentions("CC @naruto et @ymclash pour la review");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].local_handle, "naruto");
        assert_eq!(mentions[1].local_handle, "ymclash");
    }

    #[test]
    fn test_federated_mention() {
        let mentions = extract_mentions("CC @alice@mastodon.social pour info");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "alice");
        assert_eq!(mentions[0].domain, Some("mastodon.social".to_string()));
        assert!(mentions[0].is_remote());
    }

    #[test]
    fn test_mixed_local_and_federated() {
        let mentions = extract_mentions("@yusuf et @bob@forgejo.example.com merci !");
        assert_eq!(mentions.len(), 2);
        assert!(!mentions[0].is_remote());
        assert!(mentions[1].is_remote());
    }

    #[test]
    fn test_deduplication() {
        let mentions = extract_mentions("@yusuf a dit @yusuf et @yusuf encore");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "yusuf");
    }

    #[test]
    fn test_empty_text() {
        let mentions = extract_mentions("");
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_no_mentions() {
        let mentions = extract_mentions("Pas de mention ici, juste du texte.");
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_bare_at_ignored() {
        let mentions = extract_mentions("email@test.com est un email, pas une mention");
        // "email@test.com" n'est pas précédé par un espace + @
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_inline_code_ignored() {
        let mentions = extract_mentions("Utilise `@variable` dans le code");
        // @variable est dans du code inline → ignoré par pulldown-cmark
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_code_block_ignored() {
        let text = r#"Voici un exemple :

```sql
SELECT * FROM users WHERE handle = '@admin';
```

Mais @yusuf est mentionné ici."#;
        let mentions = extract_mentions(text);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "yusuf");
    }

    #[test]
    fn test_fenced_code_block_with_mentions() {
        let text = r#"```rust
let user = "@alice"; // pas une mention
```

@bob réel mentionné"#;
        let mentions = extract_mentions(text);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "bob");
    }

    #[test]
    fn test_mention_at_line_start() {
        let mentions = extract_mentions("@naruto est le premier mot");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "naruto");
    }

    #[test]
    fn test_mention_after_punctuation() {
        let mentions = extract_mentions("Voir (@yusuf) et [@bob] pour info");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].local_handle, "yusuf");
        assert_eq!(mentions[1].local_handle, "bob");
    }

    #[test]
    fn test_mention_with_hyphens_underscores() {
        let mentions = extract_mentions("CC @my-user_name pour info");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "my-user_name");
    }

    #[test]
    fn test_mention_in_blockquote() {
        let text = "> @alice a dit ceci\n\n@bob répond";
        let mentions = extract_mentions(text);
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].local_handle, "alice");
        assert_eq!(mentions[1].local_handle, "bob");
    }

    #[test]
    fn test_mention_in_list() {
        let text = "- @alice\n- @bob\n- @charlie";
        let mentions = extract_mentions(text);
        assert_eq!(mentions.len(), 3);
    }

    #[test]
    fn test_html_attribute_ignored() {
        // pulldown-cmark traite le bloc HTML comme un seul événement Html.
        // Le @test dans l'attribut est ignoré. Le @yusuf doit être sur un
        // paragraphe séparé pour être détecté comme Text par le parseur.
        let text = "<div id=\"@test\">contenu</div>\n\nmais @yusuf est réel";
        let mentions = extract_mentions(text);
        // @test est dans du HTML → ignoré
        // @yusuf est dans un paragraphe Text → détecté
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].local_handle, "yusuf");
    }
}
