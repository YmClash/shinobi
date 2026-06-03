//! # SHINOBI — Tensai : Moteur de Découpage Sémantique
//!
//! Tensai analyse le code source via Tree-sitter pour le découper
//! en fragments logiques (fonctions, classes, blocs). Ces fragments
//! nourrissent les agents IA en leur fournissant du contexte structuré
//! plutôt que du texte brut.
//!
//! ## Architecture
//! - `SemanticChunk` : fragment de code avec ses métadonnées (type, portée, dépendances)
//! - `Chunker` (Trait) : contrat de découpage, implémenté par les parsers Tree-sitter
//! - `RustChunker` : implémentation Tree-sitter pour le langage Rust
//!
//! ## Phase actuelle
//! Chunker Rust opérationnel. Langages supplémentaires en phases futures.

pub mod rust_chunker;
use serde::{Deserialize, Serialize};

/// Fragment sémantique de code source.
///
/// Représente une unité logique (fonction, struct, import, bloc)
/// extraite par le parser Tree-sitter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChunk {
    /// Type du fragment (function, struct, import, block, etc.).
    pub kind: ChunkKind,

    /// Nom du symbole (ex: "create_operation", "User").
    pub name: Option<String>,

    /// Contenu brut du fragment.
    pub content: String,

    /// Ligne de début dans le fichier source (1-indexed).
    pub start_line: usize,

    /// Ligne de fin dans le fichier source (1-indexed).
    pub end_line: usize,

    /// Chemin du fichier source.
    pub file_path: String,

    /// Langage du fichier source.
    pub language: String,
}

/// Type de fragment sémantique.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChunkKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Import,
    Module,
    Comment,
    Block,
    Unknown,
}

/// Contrat de découpage sémantique.
///
/// Sera implémenté par les parsers Tree-sitter spécifiques
/// à chaque langage (Rust, TypeScript, Python, etc.).
#[async_trait::async_trait]
pub trait Chunker: Send + Sync {
    /// Découpe le contenu source en fragments sémantiques.
    async fn chunk(
        &self,
        source: &str,
        file_path: &str,
        language: &str,
    ) -> Result<Vec<SemanticChunk>, ChunkerError>;

    /// Liste les langages supportés par ce chunker.
    fn supported_languages(&self) -> Vec<String>;
}

/// Erreurs du moteur Tensai.
#[derive(Debug, thiserror::Error)]
pub enum ChunkerError {
    #[error("Langage non supporté: {0}")]
    UnsupportedLanguage(String),

    #[error("Erreur de parsing: {0}")]
    ParseError(String),

    #[error("Erreur interne Tensai: {0}")]
    Internal(String),
}
