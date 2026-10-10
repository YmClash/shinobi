//! Port: LlmService — Contrat d'accès à un modèle de langage (LLM).
//!
//! Ce trait abstrait l'appel au LLM local (Ollama, llama.cpp, etc.).
//! L'adaptateur concret dans infrastructure/ implémente le transport HTTP réel.
//!
//! ## Phase 9 — L'Oracle Reviewer
//! Utilisé par le `ReviewOperationUseCase` pour soumettre un diff de commit
//! à un LLM et obtenir une code review automatique.
//!
//! ## Phase 15 — Sensei (先生)
//! Extension streaming : `generate_stream()` émet les tokens un par un
//! via un canal `mpsc` pour alimenter le Server-Sent Events côté API.
//!
//! ## Phase 42-pre — Hub d'Agents 🧠🔌
//! Le port devient agnostique du fournisseur (Ollama, Gemini, OpenAI-compatible).
//! Ajout de `provider()`, `list_models()` et `warmup()` avec implémentations
//! par défaut — les adaptateurs ne surchargent que ce qu'ils supportent.

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

/// Fragment de streaming LLM (Phase 15 — Sensei).
///
/// Émis token par token via un canal `mpsc::Receiver`.
/// Le frontend consomme ces fragments via Server-Sent Events.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LlmStreamChunk {
    /// Token de texte (un mot ou partie de mot).
    Token {
        content: String,
    },
    /// Fin de génération avec métadonnées.
    Done {
        model: String,
        duration_ms: u64,
    },
    /// Erreur pendant la génération.
    Error {
        message: String,
    },
}

/// Description d'un modèle disponible chez un fournisseur (Phase 42-pre).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LlmModelInfo {
    /// Identifiant du modèle (ex: "smollm2:1.7b", "gpt-4o-mini").
    pub name: String,
    /// Taille sur disque en octets (connue uniquement pour les modèles locaux).
    pub size: Option<u64>,
    /// Fournisseur (ex: "ollama", "gemini", "openai").
    pub provider: String,
}

/// Contrat d'accès à un modèle de langage (local ou distant).
///
/// Conçu pour l'inversion de dépendance : le domaine définit le contrat,
/// l'infrastructure fournit l'implémentation (Ollama, Gemini, OpenAI-compatible…).
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

    /// Génère une complétion en streaming (token par token).
    ///
    /// Phase 15 — Sensei : chaque token est émis via le canal `mpsc::Sender`.
    /// Le handler SSE consomme le `Receiver` correspondant pour streamer
    /// la réponse au frontend en temps réel.
    ///
    /// # Arguments
    /// - `prompt` : le contenu à analyser
    /// - `system` : les instructions pour le modèle
    ///
    /// # Returns
    /// Un `Receiver<LlmStreamChunk>` qui émet les tokens un par un,
    /// puis un `Done` final avec les métadonnées.
    async fn generate_stream(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<LlmStreamChunk>, DomainError>;

    /// Retourne le nom du modèle configuré (ex: "granite3.1-dense:2b").
    fn model_name(&self) -> &str;

    /// Nom du fournisseur (Phase 42-pre). Défaut : `"unknown"`.
    fn provider(&self) -> &str {
        "unknown"
    }

    /// Liste les modèles disponibles (Phase 42-pre).
    ///
    /// Défaut : uniquement le modèle configuré. Ollama surcharge via `/api/tags`.
    async fn list_models(&self) -> Result<Vec<LlmModelInfo>, DomainError> {
        Ok(vec![LlmModelInfo {
            name: self.model_name().to_string(),
            size: None,
            provider: self.provider().to_string(),
        }])
    }

    /// Pré-charge un modèle pour éliminer le cold-start (Phase 42-pre).
    ///
    /// Défaut : no-op (les API distantes n'ont pas de cold-start côté client).
    async fn warmup(&self, _model: &str) -> Result<(), DomainError> {
        Ok(())
    }
}
