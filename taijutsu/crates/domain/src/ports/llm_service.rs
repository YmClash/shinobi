//! Port: LlmService — Contrat d'accès à un modèle de langage (LLM).
//!
//! Ce trait abstrait l'appel au LLM local (Ollama, llama.cpp, etc.).
//! L'adaptateur concret dans infrastructure/ implémente le transport HTTP réel.
//!
//! ## Phase 9 — L'Oracle Reviewer
//! Utilisé par le `ReviewOperationUseCase` pour soumettre un diff de commit
//! à un LLM et obtenir une code review automatique.

use async_trait::async_trait;

use crate::errors::DomainError;

/// Réponse d'un modèle de langage (LLM).
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Contenu textuel généré par le modèle.
    pub content: String,
    /// Nom du modèle utilisé (ex: "granite3.1-dense:2b").
    pub model: String,
    /// Temps de génération en millisecondes.
    pub duration_ms: u64,
}

/// Contrat d'accès à un modèle de langage local.
///
/// Conçu pour l'inversion de dépendance : le domaine définit le contrat,
/// l'infrastructure fournit l'implémentation (Ollama HTTP, etc.).
#[async_trait]
pub trait LlmService: Send + Sync {
    /// Génère une complétion à partir d'un prompt utilisateur et d'un prompt système.
    ///
    /// # Arguments
    /// - `prompt` : le contenu à analyser (diff, code source, etc.)
    /// - `system` : les instructions pour le modèle (rôle, format de sortie)
    ///
    /// # Errors
    /// Retourne une erreur si le LLM est inaccessible ou si la génération échoue.
    async fn generate(&self, prompt: &str, system: &str) -> Result<LlmResponse, DomainError>;

    /// Retourne le nom du modèle configuré (ex: "granite3.1-dense:2b").
    fn model_name(&self) -> &str;
}
