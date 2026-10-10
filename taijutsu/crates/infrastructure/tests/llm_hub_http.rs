//! Tests HTTP des adaptateurs LLM distants (Phase 42-pre — Hub d'Agents 🧠🔌).
//!
//! Utilise `wiremock` pour simuler Gemini et les API OpenAI-compatibles :
//! mapping du prompt système, headers d'auth, streaming SSE, retry et repli.

use std::sync::Arc;
use std::time::Duration;

use domain::ports::llm_service::{LlmService, LlmStreamChunk};
use infrastructure::llm::fallback_service::FallbackLlmService;
use infrastructure::llm::gemini_service::GeminiService;
use infrastructure::llm::openai_compatible_service::OpenAiCompatibleService;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TIMEOUT: Duration = Duration::from_secs(10);

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(body.as_bytes().to_vec(), "text/event-stream")
}

async fn collect(mut rx: tokio::sync::mpsc::Receiver<LlmStreamChunk>) -> (String, Option<String>, Vec<String>) {
    let (mut text, mut done_model, mut errors) = (String::new(), None, Vec::new());
    while let Some(c) = rx.recv().await {
        match c {
            LlmStreamChunk::Token { content } => text.push_str(&content),
            LlmStreamChunk::Done { model, .. } => done_model = Some(model),
            LlmStreamChunk::Error { message } => errors.push(message),
        }
    }
    (text, done_model, errors)
}

fn openai(server: &MockServer, key: Option<&str>) -> OpenAiCompatibleService {
    OpenAiCompatibleService::new("vllm", &format!("{}/v1", server.uri()), "local-model", key, TIMEOUT).unwrap()
}

fn gemini(server: &MockServer) -> GeminiService {
    GeminiService::new("test-key", "test-model", &server.uri(), TIMEOUT).unwrap()
}

// ── OpenAI-compatible ────────────────────────────────────────────────

#[tokio::test]
async fn openai_sends_placeholder_bearer_and_system_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer not-needed"))
        .and(body_partial_json(json!({
            "model": "local-model",
            "stream": false,
            "messages": [
                { "role": "system", "content": "You are Oracle" },
                { "role": "user", "content": "review this" }
            ]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "role": "assistant", "content": "LGTM" } }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let resp = openai(&server, None).generate("review this", "You are Oracle").await.unwrap();
    assert_eq!(resp.content, "LGTM");
    assert_eq!(resp.model, "local-model");
}

#[tokio::test]
async fn openai_uses_real_key_when_configured() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "content": "ok" } }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(openai(&server, Some("sk-test")).generate("p", "s").await.unwrap().content, "ok");
}

#[tokio::test]
async fn openai_streams_sse_until_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({ "stream": true })))
        .respond_with(sse(concat!(
            ": keep-alive\n\n",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo 🥷\"}}]}\n\n",
            "data: [DONE]\n\n",
        )))
        .mount(&server)
        .await;

    let rx = openai(&server, None).generate_stream("p", "s").await.unwrap();
    let (text, done, errors) = collect(rx).await;
    assert_eq!(text, "Hello 🥷");
    assert_eq!(done.as_deref(), Some("local-model"));
    assert!(errors.is_empty());
}

#[tokio::test]
async fn openai_does_not_retry_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
        .expect(1)
        .mount(&server)
        .await;

    let err = openai(&server, Some("wrong")).generate("p", "s").await.unwrap_err();
    assert!(err.to_string().contains("401"));
}

#[tokio::test]
async fn openai_retries_once_on_503() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&server)
        .await;

    assert!(openai(&server, None).generate("p", "s").await.is_err());
}

#[tokio::test]
async fn openai_stream_reports_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .mount(&server)
        .await;

    let (text, done, errors) = collect(openai(&server, None).generate_stream("p", "s").await.unwrap()).await;
    assert!(text.is_empty());
    assert!(done.is_none());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("429"));
}

// ── Gemini ───────────────────────────────────────────────────────────

#[tokio::test]
async fn gemini_maps_system_prompt_to_system_instruction() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/test-model:generateContent"))
        .and(header("x-goog-api-key", "test-key"))
        .and(body_partial_json(json!({
            "systemInstruction": { "parts": [{ "text": "You are Oracle" }] },
            "contents": [{ "role": "user", "parts": [{ "text": "review this" }] }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{ "content": { "role": "model", "parts": [{ "text": "LG" }, { "text": "TM" }] } }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let resp = gemini(&server).generate("review this", "You are Oracle").await.unwrap();
    assert_eq!(resp.content, "LGTM");
    assert_eq!(resp.model, "test-model");
}

#[tokio::test]
async fn gemini_streams_with_alt_sse_and_no_done_sentinel() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/test-model:streamGenerateContent"))
        .and(query_param("alt", "sse"))
        .respond_with(sse(concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Kage \"}]}}]}\r\n\r\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Bunshin\"}]},\"finishReason\":\"STOP\"}]}\r\n\r\n",
        )))
        .mount(&server)
        .await;

    let (text, done, errors) = collect(gemini(&server).generate_stream("p", "s").await.unwrap()).await;
    assert_eq!(text, "Kage Bunshin");
    assert_eq!(done.as_deref(), Some("test-model"));
    assert!(errors.is_empty());
}

#[tokio::test]
async fn gemini_blocked_prompt_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "promptFeedback": { "blockReason": "SAFETY" }
        })))
        .mount(&server)
        .await;

    let err = gemini(&server).generate("p", "s").await.unwrap_err();
    assert!(err.to_string().contains("SAFETY"));
}

// ── Repli de bout en bout ────────────────────────────────────────────

#[tokio::test]
async fn fallback_switches_provider_when_primary_stream_is_rate_limited() {
    let primary_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_string("quota exceeded"))
        .mount(&primary_server)
        .await;

    let fallback_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse("data: {\"choices\":[{\"delta\":{\"content\":\"local answer\"}}]}\n\ndata: [DONE]\n\n"))
        .expect(1)
        .mount(&fallback_server)
        .await;

    let svc = FallbackLlmService::new(
        Arc::new(gemini(&primary_server)),
        Arc::new(openai(&fallback_server, None)),
    );
    assert_eq!(svc.provider(), "gemini");

    let (text, done, errors) = collect(svc.generate_stream("p", "s").await.unwrap()).await;
    assert_eq!(text, "local answer");
    assert_eq!(done.as_deref(), Some("local-model"), "Done reflects the model actually used");
    assert!(errors.is_empty());
}
