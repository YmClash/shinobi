//! Adaptateur Oracle/Sensei — Client HTTP pour Ollama (LLM local).
//!
//! Utilise `reqwest` pour appeler l'API REST d'Ollama (`POST /api/generate`).
//! Le modèle par défaut est `granite3.1-dense:2b`, configurable via env.
//!
//! ## Points critiques
//! - **Timeout explicite** : 120 secondes pour gérer les cold starts du modèle.
//! - **Retry 1×** : en cas de timeout, un seul retry automatique est tenté.
//! - **Graceful degradation** : si Ollama est down, le use case log et skip.
//!
//! ## Phase 15 — Sensei Streaming
//! `generate_stream()` utilise `stream: true` pour émettre les tokens NDJSON
//! un par un via un canal `mpsc`. La taille de contexte est forcée à 8192 tokens
//! pour supporter les prompts enrichis (RAG + Oracle + historique).
//!
//! ## API Ollama
//! ```text
//! POST /api/generate
//! {
//!   "model": "granite3.1-dense:2b",
//!   "prompt": "...",
//!   "system": "...",
//!   "stream": false  // Oracle
//!   // ou
//!   "stream": true,  // Sensei
//!   "options": { "num_ctx": 8192 }
//! }
//! ```

use std::time::{Duration, Instant};

use metrics::{counter, histogram};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use domain::errors::DomainError;
use domain::ports::llm_service::{LlmResponse, LlmService, LlmStreamChunk};

/// Client HTTP pour le serveur Ollama (LLM local).
///
/// Encapsule un `reqwest::Client` configuré avec un timeout explicite
/// de 120 secondes pour gérer les cold starts des modèles locaux.
pub struct OllamaService {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

/// Corps de la requête POST /api/generate (mode non-streaming).
#[derive(Debug, Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
}

/// Corps de la requête POST /api/generate (mode streaming, Phase 15).
#[derive(Debug, Serialize)]
struct OllamaStreamRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
    options: OllamaStreamOptions,
}

/// Options Ollama pour le streaming (Phase 15).
///
/// `num_ctx` est CRUCIAL : par défaut Ollama limite à 2048 tokens.
/// Sensei envoie ~4000-6000 tokens (fichier + RAG + Oracle + historique).
/// Sans cette option, Ollama tronque le début du prompt, rendant
/// Sensei amnésique (perd le System Prompt et le contexte RAG).
#[derive(Debug, Serialize)]
struct OllamaStreamOptions {
    num_ctx: u32,
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

/// Fragment NDJSON de l'API Ollama en mode streaming.
///
/// Chaque ligne de la réponse est un JSON indépendant :
/// `{"model":"qwen2.5-coder:7b","response":"token","done":false}`
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Champs désérialisés depuis Ollama NDJSON, pas tous lus explicitement.
struct OllamaStreamLine {
    /// Token de texte généré (vide si `done: true`).
    #[serde(default)]
    response: String,
    /// Indique la fin de la génération.
    done: bool,
    /// Durée totale (uniquement dans le dernier chunk `done: true`).
    #[serde(default)]
    total_duration: u64,
    /// Modèle utilisé.
    #[serde(default)]
    model: String,
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
            .timeout(Duration::from_secs(300)) // 5 min — cold start + long streaming
            .connect_timeout(Duration::from_secs(30))
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

    /// Appel interne à l'API Ollama avec gestion d'erreur (mode non-streaming).
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

    /// Génère une complétion en streaming (Phase 15 — Sensei).
    ///
    /// ## Flux technique
    /// 1. `POST /api/generate` avec `stream: true` + `num_ctx: 8192`
    /// 2. Ollama retourne du NDJSON (une ligne JSON par token)
    /// 3. Chaque ligne est parsée et émise via `mpsc::Sender`
    /// 4. Le handler SSE consomme le `Receiver` côté route Axum
    ///
    /// ## Gestion d'erreur
    /// - Si la connexion Ollama échoue → erreur envoyée via le canal (pas de blocage)
    /// - Si le parsing d'une ligne NDJSON échoue → warning + skip de la ligne
    /// - Si le Receiver est droppé (client déconnecté) → le spawn se termine proprement
    ///
    /// ## Architecture non-bloquante (Phase 16 fix)
    /// La requête HTTP vers Ollama est déplacée dans `tokio::spawn` pour que
    /// `generate_stream()` retourne **immédiatement** le `Receiver`. Cela
    /// permet au handler Axum d'envoyer les headers SSE instantanément,
    /// évitant les timeouts proxy pendant le cold-start du modèle (~30s).
    async fn generate_stream(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<LlmStreamChunk>, DomainError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = OllamaStreamRequest {
            model: &self.model,
            prompt,
            system,
            stream: true,
            options: OllamaStreamOptions {
                num_ctx: 4096, // smollm2:1.7b — fenêtre native, réduit le cold-start
            },
        };

        info!(
            model = %self.model,
            prompt_len = prompt.len(),
            num_ctx = 4096,
            "🥷 Sensei — Streaming LLM démarré"
        );

        // Sérialiser le body avant le spawn (lifetime issue avec &self).
        let body_json = serde_json::to_value(&body).map_err(|e| {
            DomainError::Internal(format!("Sensei — JSON serialization error: {e}"))
        })?;

        // Canal mpsc pour transmettre les tokens au handler SSE.
        let (tx, rx) = tokio::sync::mpsc::channel::<LlmStreamChunk>(128);
        let model_name = self.model.clone();
        let client = self.client.clone();

        // ── IMPORTANT : tout le I/O réseau est dans le spawn ──────────
        // Cela permet à generate_stream() de retourner immédiatement,
        // et au handler Axum d'envoyer les headers SSE au client.
        // Le proxy Next.js voit le 200 OK instantanément et attend les events.
        tokio::spawn(async move {
            use futures::StreamExt;

            let start = Instant::now();

            // Envoyer la requête et obtenir le stream de bytes.
            let response = match client
                .post(&url)
                .json(&body_json)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(LlmStreamChunk::Error {
                            message: format!("Sensei — Ollama connection failed: {e}"),
                        })
                        .await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body_text = response.text().await.unwrap_or_default();
                let _ = tx
                    .send(LlmStreamChunk::Error {
                        message: format!("Sensei — Ollama API error (HTTP {status}): {body_text}"),
                    })
                    .await;
                return;
            }

            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(LlmStreamChunk::Error {
                                message: format!("Stream read error: {e}"),
                            })
                            .await;
                        break;
                    }
                };

                // Accumuler les bytes dans un buffer et traiter ligne par ligne.
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Traiter chaque ligne NDJSON complète dans le buffer.
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<OllamaStreamLine>(&line) {
                        Ok(stream_line) => {
                            if stream_line.done {
                                // Dernier chunk — envoyer Done avec les stats.
                                let duration_ms = start.elapsed().as_millis() as u64;

                                // ── Métriques Prometheus ─────
                                histogram!(
                                    "llm_inference_duration_seconds",
                                    "model" => model_name.clone()
                                )
                                .record(start.elapsed().as_secs_f64());
                                counter!(
                                    "llm_inference_total",
                                    "model" => model_name.clone()
                                )
                                .increment(1);

                                info!(
                                    model = %model_name,
                                    duration_ms,
                                    "🥷 Sensei — Streaming terminé"
                                );

                                let _ = tx
                                    .send(LlmStreamChunk::Done {
                                        model: model_name.clone(),
                                        duration_ms,
                                    })
                                    .await;
                                return; // Stream terminé.
                            }

                            // Token normal — émettre vers le canal.
                            if !stream_line.response.is_empty() {
                                if tx
                                    .send(LlmStreamChunk::Token {
                                        content: stream_line.response,
                                    })
                                    .await
                                    .is_err()
                                {
                                    // Le Receiver a été droppé (client déconnecté).
                                    info!("🥷 Sensei — Client déconnecté, arrêt du stream");
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                line = %line,
                                error = %e,
                                "⚠️ Sensei — Ligne NDJSON invalide (ignorée)"
                            );
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
