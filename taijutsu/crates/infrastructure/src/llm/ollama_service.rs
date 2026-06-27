//! Adaptateur Oracle — Client HTTP pour Ollama (LLM local).
//!
//! Utilise `reqwest` pour appeler l'API REST d'Ollama (`POST /api/generate`).
//! Le modèle par défaut est `granite3.1-dense:2b`, configurable via env.
//!
//! ## Points critiques
//! - **Timeout explicite** : 120 secondes pour gérer les cold starts du modèle.
//! - **Retry 1×** : en cas de timeout, un seul retry automatique est tenté.
//! - **Graceful degradation** : si Ollama est down, le use case log et skip.
//!
//! ## API Ollama
//! ```text
//! POST /api/generate
//! {
//!   "model": "granite3.1-dense:2b",
//!   "prompt": "...",
//!   "system": "...",
//!   "stream": false
//! }
//! ```

use std::time::{Duration, Instant};

use metrics::{counter, histogram};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use domain::errors::DomainError;
use domain::ports::llm_service::{LlmResponse, LlmService};

/// Client HTTP pour le serveur Ollama (LLM local).
///
/// Encapsule un `reqwest::Client` configuré avec un timeout explicite
/// de 120 secondes pour gérer les cold starts des modèles locaux.
pub struct OllamaService {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

/// Corps de la requête POST /api/generate.
#[derive(Debug, Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
}

/// Réponse de l'API Ollama (mode non-streaming).
#[derive(Debug, Deserialize)]
struct OllamaApiResponse {
    /// Texte généré par le modèle.
    response: String,
    /// Nom du modèle utilisé.
    #[allow(dead_code)]
    model: String,
    /// Durée totale de génération en nanosecondes.
    #[serde(default)]
    #[allow(dead_code)]
    total_duration: u64,
}

impl OllamaService {
    /// Crée un nouveau client Ollama.
    ///
    /// # Arguments
    /// - `base_url` : URL du serveur Ollama (ex: `"http://localhost:11434"`)
    /// - `model` : nom du modèle à utiliser (ex: `"granite3.1-dense:2b"`)
    ///
    /// # Point critique
    /// Le `reqwest::Client` est construit avec un timeout de **120 secondes**
    /// pour gérer les cold starts. Sans ce timeout, un thread Tokio pourrait
    /// être gelé indéfiniment si le LLM est lent à répondre.
    pub fn new(base_url: &str, model: &str) -> Result<Self, DomainError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                DomainError::Internal(format!("Ollama HTTP client creation failed: {e}"))
            })?;

        info!(
            base_url = %base_url,
            model = %model,
            timeout_secs = 120,
            "🔮 OllamaService initialisé"
        );

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        })
    }

    /// Appel interne à l'API Ollama avec gestion d'erreur.
    async fn call_ollama(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<OllamaApiResponse, DomainError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = OllamaRequest {
            model: &self.model,
            prompt,
            system,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                DomainError::Internal(format!(
                    "Ollama HTTP request failed ({}): {e}",
                    self.model
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(DomainError::Internal(format!(
                "Ollama API error (HTTP {status}): {body_text}"
            )));
        }

        response.json::<OllamaApiResponse>().await.map_err(|e| {
            DomainError::Internal(format!("Ollama response deserialization failed: {e}"))
        })
    }
}

#[async_trait]
impl LlmService for OllamaService {
    async fn generate(&self, prompt: &str, system: &str) -> Result<LlmResponse, DomainError> {
        let start = Instant::now();

        // Premier essai.
        let result = self.call_ollama(prompt, system).await;

        // Retry 1× en cas de timeout ou erreur transitoire.
        let api_response = match result {
            Ok(resp) => resp,
            Err(e) => {
                warn!(
                    model = %self.model,
                    error = %e,
                    "⚠️ Oracle — Premier appel Ollama échoué, retry en cours..."
                );
                // Attendre 2 secondes avant le retry.
                tokio::time::sleep(Duration::from_secs(2)).await;
                self.call_ollama(prompt, system).await?
            }
        };

        let duration = start.elapsed();
        let duration_ms = duration.as_millis() as u64;
        let duration_secs = duration.as_secs_f64();

        // ── Métriques Prometheus ──────────────────────
        histogram!("llm_inference_duration_seconds", "model" => self.model.clone())
            .record(duration_secs);
        counter!("llm_inference_total", "model" => self.model.clone())
            .increment(1);

        info!(
            model = %self.model,
            response_len = api_response.response.len(),
            duration_ms,
            "🔮 Oracle — Réponse LLM reçue"
        );

        Ok(LlmResponse {
            content: api_response.response,
            model: self.model.clone(),
            duration_ms,
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
