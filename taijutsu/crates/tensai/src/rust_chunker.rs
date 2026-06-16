//! RustChunker — Implémentation Tree-sitter du Chunker pour le langage Rust.
//!
//! Parcourt l'AST produit par Tree-sitter pour extraire les fragments
//! sémantiques (fonctions, structs, enums, traits, impls, imports, modules).

use tracing::instrument;
use tree_sitter::Parser;

use crate::{ChunkKind, Chunker, ChunkerError, SemanticChunk};

/// Chunker sémantique pour le langage Rust, basé sur Tree-sitter.
pub struct RustChunker;

impl RustChunker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RustChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Chunker for RustChunker {
    #[instrument(skip(self, source))]
    async fn chunk(
        &self,
        source: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Vec<SemanticChunk>, ChunkerError> {
        if language != "rust" {
            return Err(ChunkerError::UnsupportedLanguage(language.to_string()));
        }

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|e| ChunkerError::Internal(format!("Erreur chargement parser Rust: {e}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ChunkerError::ParseError("Impossible de parser le source".into()))?;

        let root = tree.root_node();
        let mut chunks = Vec::new();

        extract_children(&root, source, file_path, &mut chunks);

        Ok(chunks)
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["rust".to_string()]
    }
}

/// Mappe un type de nœud Tree-sitter vers un `ChunkKind`.
/// Retourne `None` pour les nœuds non pertinents (whitespace, ponctuation, etc.)
fn node_kind_to_chunk_kind(kind: &str) -> Option<ChunkKind> {
    match kind {
        "function_item" => Some(ChunkKind::Function),
        "struct_item" => Some(ChunkKind::Struct),
        "enum_item" => Some(ChunkKind::Enum),
        "trait_item" => Some(ChunkKind::Trait),
        "impl_item" => Some(ChunkKind::Impl),
        "use_declaration" => Some(ChunkKind::Import),
        "mod_item" => Some(ChunkKind::Module),
        "line_comment" | "block_comment" => Some(ChunkKind::Comment),
        _ => None,
    }
}

/// Extrait le nom du symbole à partir d'un nœud AST.
///
/// Pour les fonctions, structs, enums, traits : cherche le child `name` ou `type_identifier`.
/// Pour les impls : cherche le `type_identifier` (le type implémenté).
fn extract_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // Chercher un nœud enfant nommé "name" ou "type" selon le type du parent.
    let name_node = match node.kind() {
        "function_item" => node.child_by_field_name("name"),
        "struct_item" => node.child_by_field_name("name"),
        "enum_item" => node.child_by_field_name("name"),
        "trait_item" => node.child_by_field_name("name"),
        "mod_item" => node.child_by_field_name("name"),
        "impl_item" => node.child_by_field_name("type"),
        "use_declaration" => {
            // Pour les imports, extraire le chemin complet est complexe.
            // On prend le texte brut comme nom simplifié.
            let text = node
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .trim()
                .trim_end_matches(';')
                .trim_start_matches("use ")
                .to_string();
            return Some(text);
        }
        _ => None,
    };

    name_node.and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()))
}

/// Parcourt les enfants directs d'un nœud et extrait les chunks sémantiques.
fn extract_children(
    node: &tree_sitter::Node,
    source: &str,
    file_path: &str,
    chunks: &mut Vec<SemanticChunk>,
) {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if let Some(chunk_kind) = node_kind_to_chunk_kind(child.kind()) {
            let content = child
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_string();

            let name = extract_name(&child, source);

            chunks.push(SemanticChunk {
                kind: chunk_kind,
                name,
                content,
                start_line: child.start_position().row + 1, // 1-indexed
                end_line: child.end_position().row + 1,
                file_path: file_path.to_string(),
                language: "rust".to_string(),
            });
        }

        // Récurser dans les modules et impls pour trouver les éléments imbriqués
        if matches!(child.kind(), "mod_item" | "impl_item") {
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
    async fn test_chunk_rust_function() {
        let chunker = RustChunker::new();
        let source = r#"
fn hello() {
    println!("Hello, world!");
}

struct User {
    name: String,
    age: u32,
}

enum Color {
    Red,
    Green,
    Blue,
}

impl User {
    fn new(name: String, age: u32) -> Self {
        Self { name, age }
    }
}

use std::collections::HashMap;

trait Greetable {
    fn greet(&self) -> String;
}
"#;

        let chunks = chunker.chunk(source, "test.rs", "rust").await.unwrap();

        // Vérifier qu'on trouve les éléments attendus
        let kinds: Vec<&ChunkKind> = chunks.iter().map(|c| &c.kind).collect();
        assert!(kinds.contains(&&ChunkKind::Function), "Doit trouver une fonction");
        assert!(kinds.contains(&&ChunkKind::Struct), "Doit trouver un struct");
        assert!(kinds.contains(&&ChunkKind::Enum), "Doit trouver un enum");
        assert!(kinds.contains(&&ChunkKind::Impl), "Doit trouver un impl");
        assert!(kinds.contains(&&ChunkKind::Import), "Doit trouver un import");
        assert!(kinds.contains(&&ChunkKind::Trait), "Doit trouver un trait");

        // Vérifier les noms
        let func = chunks.iter().find(|c| c.kind == ChunkKind::Function && c.name.as_deref() == Some("hello")).unwrap();
        assert!(func.content.contains("println!"));
        assert_eq!(func.language, "rust");
    }

    #[tokio::test]
    async fn test_unsupported_language() {
        let chunker = RustChunker::new();
        let result = chunker.chunk("code", "test.py", "python").await;
        assert!(result.is_err());
    }
}
