//! Port: ChunkRepository — Contrat de persistence des fragments sémantiques.
//!
//! Ce trait donne une **mémoire permanente** à l'agent IA Tensai.
//! Les fragments sont persistés dans PostgreSQL et interrogeables
//! par opération, fichier, ou nom de symbole.
//!
//! ## Pattern Hexagonal
//! Le domaine ne connaît PAS le crate `tensai` (pas de dépendance cyclique).
//! Il définit `StoredChunk` comme entité de persistence.
//! La couche application convertit `tensai::SemanticChunk` → `StoredChunk`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::DomainError;

/// Fragment sémantique persisté — entité du domaine.
///
/// Miroir simplifié de `tensai::SemanticChunk`, utilisé pour la
/// persistence sans créer de dépendance cyclique domain ↔ tensai.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChunk {
    /// Type du fragment (ex: "function", "struct", "trait").
    pub kind: String,
    /// Nom du symbole (ex: "create_operation", "User"). Nullable.
    pub name: Option<String>,
    /// Contenu brut du fragment.
    pub content: String,
    /// Ligne de début dans le fichier source (1-indexed).
    pub start_line: usize,
    /// Ligne de fin dans le fichier source (1-indexed).
    pub end_line: usize,
    /// Chemin du fichier source.
    pub file_path: String,
    /// Langage du fichier source (ex: "rust").
    pub language: String,
}

/// Contrat de persistence pour les fragments sémantiques (Mémoire IA).
///
/// Implémenté par `PostgresChunkRepository` dans la couche infrastructure.
///
/// ## Insertion
/// `save_chunks()` accepte un slice de chunks pour insertion batch.
/// L'adaptateur utilise `UNNEST` de PostgreSQL pour une performance optimale.
///
/// ## Queries
/// Toutes les queries utilisent des index dédiés pour des lookups rapides.
#[async_trait]
pub trait ChunkRepository: Send + Sync {
    /// Persiste un lot de chunks sémantiques pour une opération.
    ///
    /// Insertion batch atomique : soit tous les chunks sont insérés,
    /// soit aucun (transaction implicite PostgreSQL).
    /// Retourne le nombre de chunks insérés.
    async fn save_chunks(
        &self,
        operation_id: &Uuid,
        chunks: &[StoredChunk],
    ) -> Result<usize, DomainError>;

    /// Récupère tous les chunks d'une opération.
    ///
    /// Utilise l'index `idx_chunks_operation`.
    async fn find_by_operation(
        &self,
        operation_id: &Uuid,
    ) -> Result<Vec<StoredChunk>, DomainError>;

    /// Récupère les chunks d'un fichier spécifique dans une opération.
    ///
    /// Utilise les index `idx_chunks_operation` + `idx_chunks_file_path`.
    async fn find_by_file(
        &self,
        operation_id: &Uuid,
        file_path: &str,
    ) -> Result<Vec<StoredChunk>, DomainError>;

    /// Recherche les chunks par nom de symbole (fonction, struct, etc.).
    ///
    /// Utilise l'index partiel `idx_chunks_name` (WHERE name IS NOT NULL).
    async fn find_by_name(
        &self,
        name: &str,
    ) -> Result<Vec<StoredChunk>, DomainError>;

    /// Compte le nombre total de chunks pour une opération.
    async fn count_by_operation(
        &self,
        operation_id: &Uuid,
    ) -> Result<usize, DomainError>;
}
