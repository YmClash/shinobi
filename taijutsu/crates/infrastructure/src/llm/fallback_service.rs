//! Décorateur de repli LLM (Phase 42-pre — Hub d'Agents 🧠🔌).
//!
//! `FallbackLlmService` enveloppe un fournisseur **primaire** (API distante)
//! et un fournisseur de **repli** (Ollama local).
//!
//! ## Sémantique
//! - `generate()` : si le primaire échoue (erreur/timeout après son propre
//!   retry), on bascule sur le repli.
//! - `generate_stream()` : on bascule **uniquement si le primaire échoue avant
//!   d'avoir émis le moindre fragment** (connexion, HTTP 4xx/5xx, 429). Une fois
//!   un token parti vers le client, une erreur est propagée telle quelle — on ne
//!   mélange jamais deux réponses dans une même bulle de chat.
//! - Le `Done { model }` final reflète le modèle **réellement** utilisé.

use std::sync::Arc;

use async_trait::async_trait;
use metrics::counter;
use tokio::sync::mpsc;
use tracing::warn;

use domain::errors::DomainError;
use domain::ports::llm_service::{LlmModelInfo, LlmResponse, LlmService, LlmStreamChunk};

/// LLM primaire avec repli automatique.
pub struct FallbackLlmService {
    primary: Arc<dyn LlmService>,
    fallback: Arc<dyn LlmService>,
}

impl FallbackLlmService {
    /// Crée le décorateur `primary` → `fallback`.
    pub fn new(primary: Arc<dyn LlmService>, fallback: Arc<dyn LlmService>) -> Self {
        Self { primary, fallback }
    }
}

fn record_fallback(from: &str, to: &str) {
    counter!(
        "llm_fallback_total",
        "from" => from.to_string(),
        "to" => to.to_string()
    )
    .increment(1);
}

/// Relaie tous les fragments de `rx` vers `tx` jusqu'à épuisement ou déconnexion.
async fn forward(mut rx: mpsc::Receiver<LlmStreamChunk>, tx: &mpsc::Sender<LlmStreamChunk>) {
    while let Some(chunk) = rx.recv().await {
        if tx.send(chunk).await.is_err() {
            return; // Client déconnecté.
        }
    }
}

#[async_trait]
impl LlmService for FallbackLlmService {
    async fn generate(&self, prompt: &str, system: &str) -> Result<LlmResponse, DomainError> {
        match self.primary.generate(prompt, system).await {
            Ok(r) => Ok(r),
            Err(e) => {
                warn!(
                    primary = %self.primary.provider(),
                    fallback = %self.fallback.provider(),
                    error = %e,
                    "⚠️ Hub LLM — primaire en échec, bascule sur le repli"
                );
                record_fallback(self.primary.provider(), self.fallback.provider());
                self.fallback.generate(prompt, system).await
            }
        }
    }

    async fn generate_stream(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<mpsc::Receiver<LlmStreamChunk>, DomainError> {
        let mut primary_rx = match self.primary.generate_stream(prompt, system).await {
            Ok(rx) => rx,
            Err(e) => {
                warn!(error = %e, "⚠️ Hub LLM — stream primaire refusé, bascule sur le repli");
                record_fallback(self.primary.provider(), self.fallback.provider());
                return self.fallback.generate_stream(prompt, system).await;
            }
        };

        let (tx, rx) = mpsc::channel::<LlmStreamChunk>(128);
        let fallback = self.fallback.clone();
        let primary_provider = self.primary.provider().to_string();
        let prompt = prompt.to_string();
        let system = system.to_string();

        tokio::spawn(async move {
            match primary_rx.recv().await {
                // Échec avant le premier fragment → repli transparent.
                first @ (None | Some(LlmStreamChunk::Error { .. })) => {
                    let reason = match first {
                        Some(LlmStreamChunk::Error { message }) => message,
                        _ => "stream closed without output".to_string(),
                    };
                    warn!(
                        primary = %primary_provider,
                        fallback = %fallback.provider(),
                        reason = %reason,
                        "⚠️ Hub LLM — stream primaire en échec avant le 1er token, bascule sur le repli"
                    );
                    record_fallback(&primary_provider, fallback.provider());
                    match fallback.generate_stream(&prompt, &system).await {
                        Ok(fallback_rx) => forward(fallback_rx, &tx).await,
                        Err(e) => {
                            let _ = tx
                                .send(LlmStreamChunk::Error {
                                    message: format!("primary: {reason} | fallback: {e}"),
                                })
                                .await;
                        }
                    }
                }
                // Le primaire a démarré → on s'engage avec lui jusqu'au bout.
                Some(chunk) => {
                    if tx.send(chunk).await.is_ok() {
                        forward(primary_rx, &tx).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    fn model_name(&self) -> &str {
        self.primary.model_name()
    }

    fn provider(&self) -> &str {
        self.primary.provider()
    }

    async fn list_models(&self) -> Result<Vec<LlmModelInfo>, DomainError> {
        self.primary.list_models().await
    }

    async fn warmup(&self, model: &str) -> Result<(), DomainError> {
        self.primary.warmup(model).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock scripté : liste de fragments émis en streaming, ou erreur en `generate`.
    struct Scripted {
        name: &'static str,
        fail_generate: bool,
        stream: Vec<LlmStreamChunk>,
        calls: AtomicUsize,
    }

    impl Scripted {
        fn new(name: &'static str, fail_generate: bool, stream: Vec<LlmStreamChunk>) -> Arc<Self> {
            Arc::new(Self { name, fail_generate, stream, calls: AtomicUsize::new(0) })
        }
    }

    #[async_trait]
    impl LlmService for Scripted {
        async fn generate(&self, _p: &str, _s: &str) -> Result<LlmResponse, DomainError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_generate {
                Err(DomainError::Internal(format!("{} down", self.name)))
            } else {
                Ok(LlmResponse { content: self.name.into(), model: self.name.into(), duration_ms: 1 })
            }
        }
        async fn generate_stream(
            &self,
            _p: &str,
            _s: &str,
        ) -> Result<mpsc::Receiver<LlmStreamChunk>, DomainError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = mpsc::channel(16);
            for c in self.stream.clone() {
                tx.send(c).await.unwrap();
            }
            Ok(rx)
        }
        fn model_name(&self) -> &str {
            self.name
        }
        fn provider(&self) -> &str {
            self.name
        }
    }

    fn tok(s: &str) -> LlmStreamChunk {
        LlmStreamChunk::Token { content: s.into() }
    }
    fn done(m: &str) -> LlmStreamChunk {
        LlmStreamChunk::Done { model: m.into(), duration_ms: 1 }
    }
    fn err(m: &str) -> LlmStreamChunk {
        LlmStreamChunk::Error { message: m.into() }
    }

    async fn collect(mut rx: mpsc::Receiver<LlmStreamChunk>) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(c) = rx.recv().await {
            out.push(match c {
                LlmStreamChunk::Token { content } => format!("T:{content}"),
                LlmStreamChunk::Done { model, .. } => format!("D:{model}"),
                LlmStreamChunk::Error { .. } => "E".into(),
            });
        }
        out
    }

    #[tokio::test]
    async fn generate_uses_primary_when_ok() {
        let p = Scripted::new("gemini", false, vec![]);
        let f = Scripted::new("ollama", false, vec![]);
        let svc = FallbackLlmService::new(p.clone(), f.clone());
        assert_eq!(svc.generate("x", "y").await.unwrap().model, "gemini");
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn generate_falls_back_on_error() {
        let p = Scripted::new("gemini", true, vec![]);
        let f = Scripted::new("ollama", false, vec![]);
        let svc = FallbackLlmService::new(p, f);
        assert_eq!(svc.generate("x", "y").await.unwrap().model, "ollama");
    }

    #[tokio::test]
    async fn stream_uses_primary_when_ok() {
        let p = Scripted::new("gemini", false, vec![tok("a"), tok("b"), done("gemini")]);
        let f = Scripted::new("ollama", false, vec![tok("z"), done("ollama")]);
        let svc = FallbackLlmService::new(p, f.clone());
        let out = collect(svc.generate_stream("x", "y").await.unwrap()).await;
        assert_eq!(out, vec!["T:a", "T:b", "D:gemini"]);
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn stream_falls_back_when_primary_fails_before_first_token() {
        let p = Scripted::new("gemini", false, vec![err("HTTP 429")]);
        let f = Scripted::new("ollama", false, vec![tok("z"), done("ollama")]);
        let svc = FallbackLlmService::new(p, f);
        let out = collect(svc.generate_stream("x", "y").await.unwrap()).await;
        assert_eq!(out, vec!["T:z", "D:ollama"]);
    }

    #[tokio::test]
    async fn stream_falls_back_when_primary_closes_silently() {
        let p = Scripted::new("gemini", false, vec![]);
        let f = Scripted::new("ollama", false, vec![tok("z"), done("ollama")]);
        let svc = FallbackLlmService::new(p, f);
        let out = collect(svc.generate_stream("x", "y").await.unwrap()).await;
        assert_eq!(out, vec!["T:z", "D:ollama"]);
    }

    #[tokio::test]
    async fn stream_does_not_fall_back_after_first_token() {
        let p = Scripted::new("gemini", false, vec![tok("a"), err("connection reset")]);
        let f = Scripted::new("ollama", false, vec![tok("z"), done("ollama")]);
        let svc = FallbackLlmService::new(p, f.clone());
        let out = collect(svc.generate_stream("x", "y").await.unwrap()).await;
        assert_eq!(out, vec!["T:a", "E"]);
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    }
}
