//! Adaptateur LLM OpenAI-compatible (Phase 42-pre — Hub d'Agents 🧠🔌).
//!
//! Un seul adaptateur couvre tous les serveurs qui parlent le dialecte
//! `POST {base_url}/chat/completions` : **OpenAI**, **DeepSeek**, **vLLM**,
//! **LM Studio**, llama.cpp server…
//!
//! ## Mapping du prompt système
//! L'API OpenAI attend la consigne système comme **un message `role: "system"`**
//! en tête du tableau `messages` (contrairement à Gemini, qui utilise un champ
//! `systemInstruction` séparé).
//!
//! ## Header `Authorization` toujours présent
//! Certaines implémentations (vLLM avec `--api-key`, LM Studio, proxies) rejettent
//! les requêtes **sans** header `Authorization`, même sans clé configurée.
//! Sans clé, on envoie donc `Authorization: Bearer not-needed`.
//!
//! ## Streaming
//! `stream: true` → SSE `data: {"choices":[{"delta":{"content":"…"}}]}`
//! terminé par `data: [DONE]`.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

use domain::errors::DomainError;
use domain::ports::llm_service::{LlmResponse, LlmService, LlmStreamChunk};

use super::common::{self, CallError};
use super::sse::{self, PumpOutcome, SsePayload};

/// Jeton factice envoyé quand aucune clé n'est configurée (LM Studio, vLLM local).
pub const PLACEHOLDER_API_KEY: &str = "not-needed";

/// Client HTTP pour une API OpenAI-compatible.
///
/// ⚠️ Pas de `Debug` dérivé : la structure contient une clé d'API.
pub struct OpenAiCompatibleService {
    client: reqwest::Client,
    /// Libellé du fournisseur (ex: "openai", "deepseek", "vllm").
    provider: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChunk {
    #[serde(default)]
    choices: Vec<ChatStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    #[serde(default)]
    delta: ChatStreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct ChatStreamDelta {
    #[serde(default)]
    content: Option<String>,
}

impl OpenAiCompatibleService {
    /// Crée un client OpenAI-compatible.
    ///
    /// - `provider` : libellé pour logs/métriques (ex: `"deepseek"`)
    /// - `base_url` : racine de l'API **incluant** `/v1` (ex: `https://api.openai.com/v1`)
    /// - `api_key` : `None` ou vide → `Bearer not-needed`
    pub fn new(
        provider: &str,
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, DomainError> {
        let client = common::build_client(provider, timeout)?;
        let api_key = api_key
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string);

        info!(
            provider,
            base_url,
            model,
            api_key = if api_key.is_some() { "configured" } else { "placeholder" },
            timeout_secs = timeout.as_secs(),
            "🔌 OpenAiCompatibleService initialisé"
        );

        Ok(Self {
            client,
            provider: provider.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key,
        })
    }

    /// Jeton Bearer effectif (clé réelle ou placeholder).
    fn bearer_token(&self) -> &str {
        self.api_key.as_deref().unwrap_or(PLACEHOLDER_API_KEY)
    }

    /// Construit le corps de requête — prompt système en message `role: system`.
    fn build_body<'a>(&'a self, prompt: &'a str, system: &'a str, stream: bool) -> ChatRequest<'a> {
        let mut messages = Vec::with_capacity(2);
        if !system.trim().is_empty() {
            messages.push(ChatMessage { role: "system", content: system });
        }
        messages.push(ChatMessage { role: "user", content: prompt });
        ChatRequest { model: &self.model, messages, stream }
    }

    fn request(&self, body: &ChatRequest<'_>) -> reqwest::RequestBuilder {
        // `bearer_auth` marque le header comme sensible (masqué dans les Debug reqwest).
        self.client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(self.bearer_token())
            .json(body)
    }

    async fn call_once(&self, prompt: &str, system: &str) -> Result<String, CallError> {
        let body = self.build_body(prompt, system, false);
        let response = self
            .request(&body)
            .send()
            .await
            .map_err(|e| common::transport_error(&self.provider, &self.model, e))?;

        if !response.status().is_success() {
            return Err(common::http_error(&self.provider, response).await);
        }

        let parsed: ChatResponse = response.json().await.map_err(|e| CallError {
            message: format!("{} response deserialization failed: {e}", self.provider),
            retryable: false,
        })?;

        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| CallError {
                message: format!("{} response contained no choices", self.provider),
                retryable: false,
            })
    }
}

/// Interprète un payload SSE OpenAI.
fn parse_stream_payload(payload: &str) -> Result<SsePayload, String> {
    let payload = payload.trim();
    if payload == "[DONE]" {
        return Ok(SsePayload::Done);
    }
    let chunk: ChatStreamChunk = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    Ok(chunk
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.delta.content)
        .map(SsePayload::Token)
        .unwrap_or(SsePayload::Ignore))
}

#[async_trait]
impl LlmService for OpenAiCompatibleService {
    async fn generate(&self, prompt: &str, system: &str) -> Result<LlmResponse, DomainError> {
        let start = Instant::now();

        let content = match self.call_once(prompt, system).await {
            Ok(c) => c,
            Err(e) if e.retryable => {
                warn!(provider = %self.provider, error = %e.message, "⚠️ Hub LLM — retry en cours...");
                tokio::time::sleep(common::RETRY_DELAY).await;
                self.call_once(prompt, system).await?
            }
            Err(e) => return Err(e.into()),
        };

        common::record_inference(&self.provider, &self.model, start);

        Ok(LlmResponse {
            content,
            model: self.model.clone(),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Streaming SSE — l'I/O réseau est dans `tokio::spawn` (le `Receiver`
    /// est retourné immédiatement, cf. fix Phase 16).
    async fn generate_stream(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<mpsc::Receiver<LlmStreamChunk>, DomainError> {
        let request = self.request(&self.build_body(prompt, system, true));
        let (tx, rx) = mpsc::channel::<LlmStreamChunk>(128);
        let provider = self.provider.clone();
        let model = self.model.clone();

        tokio::spawn(async move {
            let start = Instant::now();

            let response = match request.send().await {
                Ok(r) => r,
                Err(e) => {
                    let msg = common::transport_error(&provider, &model, e).message;
                    let _ = tx.send(LlmStreamChunk::Error { message: msg }).await;
                    return;
                }
            };
            if !response.status().is_success() {
                let msg = common::http_error(&provider, response).await.message;
                let _ = tx.send(LlmStreamChunk::Error { message: msg }).await;
                return;
            }

            if sse::pump_sse(response, &tx, &provider, parse_stream_payload).await
                == PumpOutcome::Completed
            {
                common::record_inference(&provider, &model, start);
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
        &self.provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(key: Option<&str>) -> OpenAiCompatibleService {
        OpenAiCompatibleService::new("vllm", "http://x/v1/", "m", key, Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn system_prompt_is_a_system_message() {
        let s = svc(None);
        let json = serde_json::to_value(s.build_body("hello", "be terse", false)).unwrap();
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][0]["content"], "be terse");
        assert_eq!(json["messages"][1]["role"], "user");
        assert_eq!(json["messages"][1]["content"], "hello");
    }

    #[test]
    fn empty_system_prompt_is_omitted() {
        let s = svc(None);
        let json = serde_json::to_value(s.build_body("hello", "  ", false)).unwrap();
        assert_eq!(json["messages"].as_array().unwrap().len(), 1);
        assert_eq!(json["messages"][0]["role"], "user");
    }

    #[test]
    fn placeholder_bearer_when_no_key() {
        assert_eq!(svc(None).bearer_token(), PLACEHOLDER_API_KEY);
        assert_eq!(svc(Some("   ")).bearer_token(), PLACEHOLDER_API_KEY);
        assert_eq!(svc(Some("sk-123")).bearer_token(), "sk-123");
    }

    #[test]
    fn base_url_trailing_slash_is_trimmed() {
        assert_eq!(svc(None).base_url, "http://x/v1");
    }

    #[test]
    fn parses_stream_payloads() {
        assert_eq!(parse_stream_payload("[DONE]").unwrap(), SsePayload::Done);
        assert_eq!(
            parse_stream_payload(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#).unwrap(),
            SsePayload::Token("Hi".into())
        );
        assert_eq!(
            parse_stream_payload(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#).unwrap(),
            SsePayload::Ignore
        );
        assert_eq!(parse_stream_payload(r#"{"choices":[]}"#).unwrap(), SsePayload::Ignore);
        assert!(parse_stream_payload("not json").is_err());
    }
}
