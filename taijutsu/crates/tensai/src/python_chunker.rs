//! PythonChunker — Tree-sitter Chunker for Python.

use tracing::instrument;
use tree_sitter::Parser;
use crate::{ChunkKind, Chunker, ChunkerError, SemanticChunk};

pub struct PythonChunker;

impl PythonChunker {
    pub fn new() -> Self { Self }
}

impl Default for PythonChunker {
    fn default() -> Self { Self::new() }
}

#[async_trait::async_trait]
impl Chunker for PythonChunker {
    #[instrument(skip(self, source))]
    async fn chunk(&self, source: &str, file_path: &str, language: &str) -> Result<Vec<SemanticChunk>, ChunkerError> {
        if language != "python" {
            return Err(ChunkerError::UnsupportedLanguage(language.to_string()));
        }
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| ChunkerError::Internal(format!("Python parser error: {e}")))?;
        let tree = parser.parse(source, None)
            .ok_or_else(|| ChunkerError::ParseError("Cannot parse Python".into()))?;
        let mut chunks = Vec::new();
        extract_children(&tree.root_node(), source, file_path, &mut chunks);
        Ok(chunks)
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["python".to_string()]
    }
}

fn node_kind_to_chunk_kind(kind: &str) -> Option<ChunkKind> {
    match kind {
        "function_definition" => Some(ChunkKind::Function),
        "class_definition" => Some(ChunkKind::Struct),
        "import_statement" | "import_from_statement" => Some(ChunkKind::Import),
        "decorated_definition" => None, // We'll handle the inner definition
        "comment" => Some(ChunkKind::Comment),
        _ => None,
    }
}

fn extract_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" | "class_definition" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()))
        }
        "import_statement" | "import_from_statement" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or_default().trim().to_string();
            Some(text)
        }
        _ => None,
    }
}

fn extract_children(node: &tree_sitter::Node, source: &str, file_path: &str, chunks: &mut Vec<SemanticChunk>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Handle decorated definitions (e.g. @staticmethod def foo)
        if child.kind() == "decorated_definition" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if let Some(kind) = node_kind_to_chunk_kind(inner.kind()) {
                    // Use the full decorated definition as content, but the inner definition's name
                    chunks.push(SemanticChunk {
                        kind,
                        name: extract_name(&inner, source),
                        content: child.utf8_text(source.as_bytes()).unwrap_or_default().to_string(),
                        start_line: child.start_position().row + 1,
                        end_line: child.end_position().row + 1,
                        file_path: file_path.to_string(),
                        language: "python".to_string(),
                    });
                }
            }
            continue;
        }

        if let Some(chunk_kind) = node_kind_to_chunk_kind(child.kind()) {
            chunks.push(SemanticChunk {
                kind: chunk_kind,
                name: extract_name(&child, source),
                content: child.utf8_text(source.as_bytes()).unwrap_or_default().to_string(),
                start_line: child.start_position().row + 1,
                end_line: child.end_position().row + 1,
                file_path: file_path.to_string(),
                language: "python".to_string(),
            });
        }

        // Recurse into class bodies
        if child.kind() == "class_definition" {
            if let Some(body) = child.child_by_field_name("body") {
                extract_children(&body, source, file_path, chunks);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chunk_python_class() {
        let chunker = PythonChunker::new();
        let source = "import os\nfrom pathlib import Path\n\nclass User:\n    def __init__(self, name: str):\n        self.name = name\n\n    def greet(self) -> str:\n        return f\"Hello, {self.name}!\"\n\ndef main():\n    user = User(\"Alice\")\n    print(user.greet())\n";
        let chunks = chunker.chunk(source, "main.py", "python").await.unwrap();
        let kinds: Vec<&ChunkKind> = chunks.iter().map(|c| &c.kind).collect();
        assert!(kinds.contains(&&ChunkKind::Import), "Import");
        assert!(kinds.contains(&&ChunkKind::Struct), "Class → Struct");
        assert!(kinds.contains(&&ChunkKind::Function), "Function");
        let main_fn = chunks.iter().find(|c| c.kind == ChunkKind::Function && c.name.as_deref() == Some("main"));
        assert!(main_fn.is_some(), "Must find main function");
    }

    #[tokio::test]
    async fn test_unsupported_language() {
        let chunker = PythonChunker::new();
        assert!(chunker.chunk("x", "t.rs", "rust").await.is_err());
    }
}
