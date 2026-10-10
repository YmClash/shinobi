//! Adaptateur LLM Google Gemini (Phase 42-pre — Hub d'Agents 🧠🔌).
//!
//! ## Endpoints
//! ```text
//! POST {base}/v1beta/models/{model}:generateContent
//! POST {base}/v1beta/models/{model}:streamGenerateContent?alt=sse
//! Header : x-goog-api-key: <clé>
//! ```
//!
//! ## Mapping du prompt système (« Piège de l'Isolation de Contexte »)
//! Gemini attend la consigne système dans le champ **séparé**
//! `systemInstruction: { parts: [{ text }] }` — **pas** comme un message de
//! `contents` (il n'existe pas de rôle `system` dans `contents` : seuls
//! `user` et `model` sont valides). Le prompt utilisateur va dans
//! `contents: [{ role: "user", parts: [{ text }] }]`.
//!
//! ## Streaming
//! Avec `?alt=sse`, chaque événement `data:` est un `GenerateContentResponse`
//! complet. Il n'y a **pas** de sentinelle `[DONE]` : la fin du body = fin du flux.
//!
//! ## Modèle
//! Aucun nom de modèle n'est codé en dur : `GEMINI_MODEL` est obligatoire
//! dès que `GEMINI_API_KEY` est renseignée (validé dans `config.rs`).

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

use domain::errors::DomainError;
use domain::ports::llm_service::{LlmResponse, LlmService, LlmStreamChunk};

use super::common::{self, CallError};
use super::sse::{self, PumpOutcome, SsePayload};

/// URL racine par défaut de l'API Gemini.
pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";

const PROVIDER: &str = "gemini";

/// Client HTTP pour l'API Gemini.
///
/// ⚠️ Pas de `Debug` dérivé : la structure contient une clé d'API.
pub struct GeminiService {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
}

// ── Requête ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct Part<'a> {
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct SystemInstruction<'a> {
    parts: Vec<Part<'a>>,
}

#[derive(Debug, Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<Part<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction<'a>>,
    contents: Vec<Content<'a>>,
}

// ── Réponse ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(default)]
    prompt_feedback: Option<PromptFeedback>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    #[serde(default)]
    content: Option<CandidateContent>,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    #[serde(default)]
    parts: Vec<ResponsePart>,
}

#[derive(Debug, Deserialize)]
struct ResponsePart {
    #[serde(default)]
    text: Option<String>,
    /// Parties de « réflexion » (modèles thinking) — exclues de la sortie.
    #[serde(default)]
    thought: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    #[serde(default)]
    block_reason: Option<String>,
}

impl GenerateResponse {
    /// Concatène le texte du premier candidat (hors parties « thought »).
    fn text(&self) -> String {
        self.candidates
            .first()
            .and_then(|c| c.content.as_ref())
            .map(|c| {
                c.parts
                    .iter()
                    .filter(|p| p.thought != Some(true))
                    .filter_map(|p| p.text.as_deref())
                    .collect::<String>()
            })
            .unwrap_or_default()
    }

    fn block_reason(&self) -> Option<&str> {
        self.prompt_feedback.as_ref().and_then(|f| f.block_reason.as_deref())
    }
}

impl GeminiService {
    /// Crée un client Gemini.
    ///
    /// - `model` : identifiant du modèle (le préfixe `models/` est toléré)
    /// - `base_url` : `DEFAULT_GEMINI_BASE_URL` en production (surchargeable pour les tests)
    pub fn new(
        api_key: &str,
        model: &str,
        base_url: &str,
        timeout: Duration,
    ) -> Result<Self, DomainError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(DomainError::Internal("GEMINI_API_KEY is empty".into()));
        }
        let model = model.trim().trim_start_matches("models/");
        if model.is_empty() {
            return Err(DomainError::Internal("GEMINI_MODEL is required".into()));
        }

        let client = common::build_client(PROVIDER, timeout)?;
        info!(model, timeout_secs = timeout.as_secs(), "🔌 GeminiService initialisé");

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
        })
    }

    /// Construit le corps — prompt système dans `systemInstruction` (champ séparé).
    fn build_body<'a>(prompt: &'a str, system: &'a str) -> GenerateRequest<'a> {
        let system_instruction = (!system.trim().is_empty()).then(|| SystemInstruction {
            parts: vec![Part { text: system }],
        });
        GenerateRequest {
            system_instruction,
            contents: vec![Content {
                role: "user",
                parts: vec![Part { text: prompt }],
            }],
        }
    }

    fn request(&self, method: &str, prompt: &str, system: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/v1beta/models/{}:{method}", self.base_url, self.model);
        let mut key = reqwest::header::HeaderValue::from_str(&self.api_key)
            .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("invalid"));
        key.set_sensitive(true);
        self.client
            .post(url)
            .header("x-goog-api-key", key)
            .json(&Self::build_body(prompt, system))
    }

    async fn call_once(&self, prompt: &str, system: &str) -> Result<String, CallError> {
        let response = self
            .request("generateContent", prompt, system)
            .send()
            .await
            .map_err(|e| common::transport_error(PROVIDER, &self.model, e))?;

        if !response.status().is_success() {
            return Err(common::http_error(PROVIDER, response).await);
        }

        let parsed: GenerateResponse = response.json().await.map_err(|e| CallError {
            message: format!("gemini response deserialization failed: {e}"),
            retryable: false,
        })?;

        if let Some(reason) = parsed.block_reason() {
            return Err(CallError {
                message: format!("gemini blocked the prompt: {reason}"),
                retryable: false,
            });
        }
        if parsed.candidates.is_empty() {
            return Err(CallError {
                message: "gemini response contained no candidates".into(),
                retryable: false,
            });
        }
        Ok(parsed.text())
    }
}

/// Interprète un payload SSE Gemini (un `GenerateContentResponse` par événement).
fn parse_stream_payload(payload: &str) -> Result<SsePayload, String> {
    let chunk: GenerateResponse = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    if let Some(reason) = chunk.block_reason() {
        return Err(format!("prompt blocked: {reason}"));
    }
    let text = chunk.text();
    Ok(if text.is_empty() { SsePayload::Ignore } else { SsePayload::Token(text) })
}

#[async_trait]
impl LlmService for GeminiService {
    async fn generate(&self, prompt: &str, system: &str) -> Result<LlmResponse, DomainError> {
        let start = Instant::now();

        let content = match self.call_once(prompt, system).await {
            Ok(c) => c,
            Err(e) if e.retryable => {
                warn!(provider = PROVIDER, error = %e.message, "⚠️ Hub LLM — retry en cours...");
                tokio::time::sleep(common::RETRY_DELAY).await;
                self.call_once(prompt, system).await?
            }
            Err(e) => return Err(e.into()),
        };

        common::record_inference(PROVIDER, &self.model, start);

        Ok(LlmResponse {
            content,
            model: self.model.clone(),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn generate_stream(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<mpsc::Receiver<LlmStreamChunk>, DomainError> {
        let request = self
            .request("streamGenerateContent", prompt, system)
            .query(&[("alt", "sse")]);
        let (tx, rx) = mpsc::channel::<LlmStreamChunk>(128);
        let model = self.model.clone();

        tokio::spawn(async move {
            let start = Instant::now();

            let response = match request.send().await {
                Ok(r) => r,
                Err(e) => {
                    let msg = common::transport_error(PROVIDER, &model, e).message;
                    let _ = tx.send(LlmStreamChunk::Error { message: msg }).await;
                    return;
                }
            };
            if !response.status().is_success() {
                let msg = common::http_error(PROVIDER, response).await.message;
                let _ = tx.send(LlmStreamChunk::Error { message: msg }).await;
                return;
            }

            if sse::pump_sse(response, &tx, PROVIDER, parse_stream_payload).await
                == PumpOutcome::Completed
            {
                common::record_inference(PROVIDER, &model, start);
                let _ = tx
                    .send(LlmStreamChunk::Done {
                        model,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                    .await;
            }
        });

        Ok(rx)
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn provider(&self) -> &str {
        PROVIDER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_goes_to_system_instruction_not_contents() {
        let json = serde_json::to_value(GeminiService::build_body("diff…", "You are Oracle")).unwrap();
        assert_eq!(json["systemInstruction"]["parts"][0]["text"], "You are Oracle");
        let contents = json["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "diff…");
        // Aucun rôle `system` dans `contents` (invalide pour Gemini).
        assert!(contents.iter().all(|c| c["role"] != "system"));
    }

    #[test]
    fn empty_system_prompt_omits_field() {
        let json = serde_json::to_value(GeminiService::build_body("hi", "")).unwrap();
        assert!(json.get("systemInstruction").is_none());
    }

    #[test]
    fn model_prefix_is_tolerated() {
        let s = GeminiService::new("k", "models/foo", "http://x/", Duration::from_secs(1)).unwrap();
        assert_eq!(s.model, "foo");
        assert_eq!(s.base_url, "http://x");
    }

    #[test]
    fn missing_model_or_key_is_rejected() {
        assert!(GeminiService::new("k", " ", "http://x", Duration::from_secs(1)).is_err());
        assert!(GeminiService::new("", "m", "http://x", Duration::from_secs(1)).is_err());
    }

    #[test]
    fn parses_stream_payload_and_skips_thoughts() {
        let p = r#"{"candidates":[{"content":{"role":"model","parts":[
            {"text":"thinking…","thought":true},{"text":"Hello"},{"text":" world"}]}}]}"#;
        assert_eq!(parse_stream_payload(p).unwrap(), SsePayload::Token("Hello world".into()));
        assert_eq!(
            parse_stream_payload(r#"{"candidates":[{"finishReason":"STOP"}]}"#).unwrap(),
            SsePayload::Ignore
        );
        assert!(parse_stream_payload(r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#).is_err());
    }
}
