//! TypeScriptChunker — Implémentation Tree-sitter du Chunker pour TypeScript et TSX.
//!
//! Parcourt l'AST produit par Tree-sitter pour extraire les fragments
//! sémantiques (fonctions, interfaces, types, classes, enums, imports).
//!
//! Supporte les deux dialectes : `typescript` (`.ts`) et `tsx` (`.tsx`).

use tracing::instrument;
use tree_sitter::Parser;

use crate::{ChunkKind, Chunker, ChunkerError, SemanticChunk};

/// Chunker sémantique pour TypeScript/TSX, basé sur Tree-sitter.
pub struct TypeScriptChunker;

impl TypeScriptChunker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TypeScriptChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Chunker for TypeScriptChunker {
    #[instrument(skip(self, source))]
    async fn chunk(
        &self,
        source: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Vec<SemanticChunk>, ChunkerError> {
        let lang_fn = match language {
            "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            "tsx" => tree_sitter_typescript::LANGUAGE_TSX,
            _ => return Err(ChunkerError::UnsupportedLanguage(language.to_string())),
        };

        let mut parser = Parser::new();
        parser
            .set_language(&lang_fn.into())
            .map_err(|e| ChunkerError::Internal(format!("Erreur chargement parser TS/TSX: {e}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ChunkerError::ParseError("Impossible de parser le source TS/TSX".into()))?;

        let root = tree.root_node();
        let mut chunks = Vec::new();

        extract_children(&root, source, file_path, language, &mut chunks);

        Ok(chunks)
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["typescript".to_string(), "tsx".to_string()]
    }
}

/// Mappe un type de nœud Tree-sitter TypeScript/TSX vers un `ChunkKind`.
fn node_kind_to_chunk_kind(kind: &str) -> Option<ChunkKind> {
    match kind {
        // Functions
        "function_declaration" | "generator_function_declaration" => Some(ChunkKind::Function),
        "method_definition" => Some(ChunkKind::Function),
        "arrow_function" => Some(ChunkKind::Function),

        // Types & Interfaces → mapped to semantic equivalents
        "interface_declaration" => Some(ChunkKind::Trait),
        "type_alias_declaration" => Some(ChunkKind::Struct),
        "class_declaration" => Some(ChunkKind::Struct),
        "enum_declaration" => Some(ChunkKind::Enum),
        "abstract_class_declaration" => Some(ChunkKind::Trait),

        // Imports / Exports
        "import_statement" => Some(ChunkKind::Import),
        "export_statement" => Some(ChunkKind::Module),

        // Comments
        "comment" => Some(ChunkKind::Comment),

        // Top-level variable declarations (const/let/var)
        "lexical_declaration" | "variable_declaration" => Some(ChunkKind::Block),

        _ => None,
    }
}

/// Extrait le nom du symbole à partir d'un nœud AST TypeScript.
fn extract_name(node: &tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()))
        }
        "class_declaration" | "abstract_class_declaration" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()))
        }
        "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()))
        }
        "method_definition" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()))
        }
        "arrow_function" => {
            // Arrow functions often have no name; try parent (variable_declarator).
            None
        }
        "lexical_declaration" | "variable_declaration" => {
            // Extract the first declarator name.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        return name_node.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
                    }
                }
            }
            None
        }
        "import_statement" => {
            // Simplify: return the import source string.
            node.child_by_field_name("source")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(|s| s.trim_matches('\'').trim_matches('"').to_string()))
        }
        "export_statement" => {
            // Try to get the name of the exported declaration.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(name) = extract_name(&child, source) {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Parcourt les enfants directs d'un nœud et extrait les chunks sémantiques.
fn extract_children(
    node: &tree_sitter::Node,
    source: &str,
    file_path: &str,
    language: &str,
    chunks: &mut Vec<SemanticChunk>,
) {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        // Special case: export_statement wraps a declaration.
        // We want to extract the inner declaration AND the export itself.
        if child.kind() == "export_statement" {
            // Extract the export as a Module chunk
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
                    start_line: child.start_position().row + 1,
                    end_line: child.end_position().row + 1,
                    file_path: file_path.to_string(),
                    language: language.to_string(),
                });
            }

            // Also recurse into the export to find inner declarations
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if inner_child.kind() != "export_statement" {
                    if let Some(inner_kind) = node_kind_to_chunk_kind(inner_child.kind()) {
                        let content = inner_child
                            .utf8_text(source.as_bytes())
                            .unwrap_or_default()
                            .to_string();
                        let name = extract_name(&inner_child, source);

                        chunks.push(SemanticChunk {
                            kind: inner_kind,
                            name,
                            content,
                            start_line: inner_child.start_position().row + 1,
                            end_line: inner_child.end_position().row + 1,
                            file_path: file_path.to_string(),
                            language: language.to_string(),
                        });
                    }
                }
            }
            continue;
        }

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
                start_line: child.start_position().row + 1,
                end_line: child.end_position().row + 1,
                file_path: file_path.to_string(),
                language: language.to_string(),
            });
        }

        // Recurse into class bodies to find methods
        if child.kind() == "class_declaration" || child.kind() == "abstract_class_declaration" {
            if let Some(body) = child.child_by_field_name("body") {
                extract_children(&body, source, file_path, language, chunks);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chunk_typescript_function() {
        let chunker = TypeScriptChunker::new();
        let source = r#"
import { useState } from "react";

interface User {
    name: string;
    age: number;
}

type Config = {
    debug: boolean;
};

function greet(user: User): string {
    return `Hello, ${user.name}!`;
}

enum Color {
    Red = "red",
    Blue = "blue",
}

class UserService {
    private users: User[] = [];

    addUser(user: User): void {
        this.users.push(user);
    }
}

const PI = 3.14159;
"#;

        let chunks = chunker.chunk(source, "test.ts", "typescript").await.unwrap();
        let kinds: Vec<&ChunkKind> = chunks.iter().map(|c| &c.kind).collect();

        assert!(kinds.contains(&&ChunkKind::Import), "Doit trouver un import");
        assert!(kinds.contains(&&ChunkKind::Trait), "Doit trouver une interface (→ Trait)");
        assert!(kinds.contains(&&ChunkKind::Struct), "Doit trouver un type alias (→ Struct)");
        assert!(kinds.contains(&&ChunkKind::Function), "Doit trouver une function");
        assert!(kinds.contains(&&ChunkKind::Enum), "Doit trouver un enum");

        // Verify names
        let func = chunks.iter().find(|c| c.kind == ChunkKind::Function && c.name.as_deref() == Some("greet")).unwrap();
        assert!(func.content.contains("Hello"));
        assert_eq!(func.language, "typescript");
    }

    #[tokio::test]
    async fn test_chunk_tsx_component() {
        let chunker = TypeScriptChunker::new();
        let source = r#"
import React from "react";

interface ButtonProps {
    label: string;
    onClick: () => void;
}

export function Button({ label, onClick }: ButtonProps) {
    return <button onClick={onClick}>{label}</button>;
}
"#;

        let chunks = chunker.chunk(source, "Button.tsx", "tsx").await.unwrap();
        let kinds: Vec<&ChunkKind> = chunks.iter().map(|c| &c.kind).collect();

        assert!(kinds.contains(&&ChunkKind::Import), "Doit trouver un import");
        assert!(kinds.contains(&&ChunkKind::Trait), "Doit trouver une interface");

        // The export statement should be found
        let has_button = chunks.iter().any(|c| {
            c.content.contains("Button")
        });
        assert!(has_button, "Doit contenir le composant Button");
    }

    #[tokio::test]
    async fn test_unsupported_language() {
        let chunker = TypeScriptChunker::new();
        let result = chunker.chunk("code", "test.py", "python").await;
        assert!(result.is_err());
    }
}
