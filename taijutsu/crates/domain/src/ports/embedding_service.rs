//! Port: EmbeddingService — Contrat d'embedding vectoriel.
//!
//! Ce trait abstrait le moteur d'embedding sous-jacent (Nomic, MiniLM, etc.).
//! L'adaptateur concret dans infrastructure/ implémente la génération
//! d'embeddings via ONNX Runtime (fastembed).
//!
//! ## Pattern Hexagonal
//! Le domaine ne connaît PAS le modèle d'embedding ni le runtime ONNX.
//! Il définit uniquement le contrat : texte → vecteur de flottants.
//!
//! ## Phase 7A
//! Implémenté par `NomicEmbedService` (Nomic-Embed-Text-v1.5, 256d Matryoshka).

use async_trait::async_trait;

use crate::errors::DomainError;

/// Contrat d'embedding vectoriel pour la recherche sémantique (RAG).
///
/// Transforme du texte en vecteurs mathématiques (embeddings) permettant
/// la recherche par similarité cosinus dans PostgreSQL (pgvector).
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// Génère un embedding pour un texte unique.
    ///
    /// Utilisé pour la recherche : le texte de la requête utilisateur
    /// est transformé en vecteur, puis comparé aux embeddings stockés.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, DomainError>;

    /// Génère des embeddings pour un batch de textes (optimisé).
    ///
    /// Utilisé lors de l'indexation : tous les chunks d'une opération
    /// sont vectorisés en un seul appel ONNX pour maximiser le throughput.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, DomainError>;

    /// Retourne le nombre de dimensions des embeddings générés.
    ///
    /// Utilisé pour la validation (doit correspondre à `vector(N)` en SQL).
    fn dimensions(&self) -> usize;
}
