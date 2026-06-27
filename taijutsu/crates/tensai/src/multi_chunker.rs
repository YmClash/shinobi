//! MultiChunker — Dispatches to the correct language-specific chunker.
//!
//! Replaces `RustChunker` as the single injected `Arc<dyn Chunker>` in the DI container.
//! Internally maintains one instance of each language chunker and delegates based on language.

use crate::{Chunker, ChunkerError, SemanticChunk};
use crate::css_chunker::CssChunker;
use crate::python_chunker::PythonChunker;
use crate::rust_chunker::RustChunker;
use crate::typescript_chunker::TypeScriptChunker;

/// Polyglot chunker that dispatches to the right language-specific parser.
///
/// Supported languages: rust, typescript, tsx, css, python.
pub struct MultiChunker {
    rust: RustChunker,
    typescript: TypeScriptChunker,
    css: CssChunker,
    python: PythonChunker,
}

impl MultiChunker {
    pub fn new() -> Self {
        Self {
            rust: RustChunker::new(),
            typescript: TypeScriptChunker::new(),
            css: CssChunker::new(),
            python: PythonChunker::new(),
        }
    }
}

impl Default for MultiChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Chunker for MultiChunker {
    async fn chunk(
        &self,
        source: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Vec<SemanticChunk>, ChunkerError> {
        match language {
            "rust" => self.rust.chunk(source, file_path, language).await,
            "typescript" | "tsx" => self.typescript.chunk(source, file_path, language).await,
            "css" => self.css.chunk(source, file_path, language).await,
            "python" => self.python.chunk(source, file_path, language).await,
            _ => Err(ChunkerError::UnsupportedLanguage(language.to_string())),
        }
    }

    fn supported_languages(&self) -> Vec<String> {
        vec![
            "rust".to_string(),
            "typescript".to_string(),
            "tsx".to_string(),
            "css".to_string(),
            "python".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_multi_chunker_dispatch() {
        let chunker = MultiChunker::new();

        // Rust
        let rust_chunks = chunker.chunk("fn hello() {}", "test.rs", "rust").await.unwrap();
        assert!(!rust_chunks.is_empty(), "Must parse Rust");

        // TypeScript
        let ts_chunks = chunker.chunk("function greet(): void {}", "test.ts", "typescript").await.unwrap();
        assert!(!ts_chunks.is_empty(), "Must parse TypeScript");

        // CSS
        let css_chunks = chunker.chunk(".foo { color: red; }", "test.css", "css").await.unwrap();
        assert!(!css_chunks.is_empty(), "Must parse CSS");

        // Python
        let py_chunks = chunker.chunk("def hello():\n    pass", "test.py", "python").await.unwrap();
        assert!(!py_chunks.is_empty(), "Must parse Python");
    }

    #[tokio::test]
    async fn test_multi_chunker_unsupported() {
        let chunker = MultiChunker::new();
        let result = chunker.chunk("code", "test.go", "go").await;
        assert!(result.is_err(), "Go should be unsupported");
    }
}
