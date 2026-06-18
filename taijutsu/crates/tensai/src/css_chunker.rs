//! CssChunker — Tree-sitter Chunker for CSS.

use tracing::instrument;
use tree_sitter::Parser;
use crate::{ChunkKind, Chunker, ChunkerError, SemanticChunk};

pub struct CssChunker;

impl CssChunker {
    pub fn new() -> Self { Self }
}

impl Default for CssChunker {
    fn default() -> Self { Self::new() }
}

#[async_trait::async_trait]
impl Chunker for CssChunker {
    #[instrument(skip(self, source))]
    async fn chunk(&self, source: &str, file_path: &str, language: &str) -> Result<Vec<SemanticChunk>, ChunkerError> {
        if language != "css" {
            return Err(ChunkerError::UnsupportedLanguage(language.to_string()));
        }
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_css::LANGUAGE.into())
            .map_err(|e| ChunkerError::Internal(format!("CSS parser error: {e}")))?;
        let tree = parser.parse(source, None)
            .ok_or_else(|| ChunkerError::ParseError("Cannot parse CSS".into()))?;
        let mut chunks = Vec::new();
        extract_children(&tree.root_node(), source, file_path, &mut chunks);
        Ok(chunks)
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["css".to_string()]
    }
}

fn node_kind_to_chunk_kind(kind: &str) -> Option<ChunkKind> {
    match kind {
        "rule_set" => Some(ChunkKind::Block),
        "at_rule" | "media_statement" | "keyframes_statement" | "import_statement"
        | "charset_statement" | "namespace_statement" | "supports_statement" => Some(ChunkKind::Module),
        "comment" => Some(ChunkKind::Comment),
        _ => None,
    }
}

fn extract_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "rule_set" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "selectors" {
                    return child.utf8_text(source.as_bytes()).ok()
                        .map(|s| { let s = s.trim(); if s.len() > 60 { format!("{}…", &s[..57]) } else { s.to_string() } });
                }
            }
            None
        }
        _ => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or_default();
            let first_line = text.lines().next().unwrap_or("").trim();
            Some(first_line.trim_end_matches('{').trim().to_string())
        }
    }
}

fn extract_children(node: &tree_sitter::Node, source: &str, file_path: &str, chunks: &mut Vec<SemanticChunk>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(chunk_kind) = node_kind_to_chunk_kind(child.kind()) {
            chunks.push(SemanticChunk {
                kind: chunk_kind,
                name: extract_name(&child, source),
                content: child.utf8_text(source.as_bytes()).unwrap_or_default().to_string(),
                start_line: child.start_position().row + 1,
                end_line: child.end_position().row + 1,
                file_path: file_path.to_string(),
                language: "css".to_string(),
            });
        }
        // Recurse into @media etc.
        if matches!(child.kind(), "at_rule" | "media_statement" | "keyframes_statement" | "supports_statement") {
            let mut inner = child.walk();
            for ic in child.children(&mut inner) {
                if ic.kind() == "block" { extract_children(&ic, source, file_path, chunks); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chunk_css_rules() {
        let chunker = CssChunker::new();
        let source = "/* Reset */\n* { margin: 0; }\n.container { max-width: 1200px; }\n@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }";
        let chunks = chunker.chunk(source, "styles.css", "css").await.unwrap();
        let kinds: Vec<&ChunkKind> = chunks.iter().map(|c| &c.kind).collect();
        assert!(kinds.contains(&&ChunkKind::Comment), "Comment");
        assert!(kinds.contains(&&ChunkKind::Block), "Rule set");
        assert!(kinds.contains(&&ChunkKind::Module), "@keyframes");
    }

    #[tokio::test]
    async fn test_unsupported_language() {
        let chunker = CssChunker::new();
        assert!(chunker.chunk("x", "t.rs", "rust").await.is_err());
    }
}
