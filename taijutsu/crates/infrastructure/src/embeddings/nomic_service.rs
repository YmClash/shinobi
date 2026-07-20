//! Adaptateur RAG — Service d'embedding Nomic via fastembed.
//!
//! Utilise `fastembed` (ONNX Runtime local) pour générer des embeddings
//! vectoriels avec le modèle **Nomic-Embed-Text-v1.5**.
//!
//! ## Matryoshka Representation Learning
//! Le modèle génère nativement des vecteurs 768d. On tronque à `dimensions`
//! (configurable, défaut 256) puis on re-normalise (L2) pour maintenir
//! la qualité de la similarité cosinus.
//!
//! ## Task Prefixes
//! Nomic utilise des préfixes pour distinguer l'indexation de la recherche :
//! - Indexation : `search_document: {text}`
//! - Recherche : `search_query: {text}`
//!
//! **L'appelant est responsable du préfixage** — ce service transmet
//! les textes tels quels au modèle.

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Mutex;
use tracing::{info, instrument};

use domain::errors::DomainError;
use domain::ports::embedding_service::EmbeddingService;

/// Service d'embedding Nomic-Embed-Text-v1.5 via ONNX Runtime local.
///
/// Encapsule le modèle `TextEmbedding` de fastembed dans un `Mutex`
/// car l'API fastembed n'est pas thread-safe (session ONNX).
pub struct NomicEmbedService {
    /// Modèle fastembed (protégé par Mutex — session ONNX non Send).
    model: Mutex<TextEmbedding>,
    /// Nombre de dimensions cible (Matryoshka truncation).
    dimensions: usize,
}

impl NomicEmbedService {
    /// Initialise le service d'embedding Nomic.
    ///
    /// Le modèle ONNX (~275 MB) est téléchargé au premier appel
    /// dans le cache fastembed local.
    ///
    /// # Arguments
    /// - `dimensions` : nombre de dimensions cible (64-768, recommandé: 256)
    pub fn new(dimensions: usize) -> Result<Self, DomainError> {
        // Phase 21 — Anti-OOM : Limiter ONNX à 2 threads pour protéger
        // la RAM/CPU sur machines 16GB (Ollama + Kafka + PostgreSQL cohabitent).
        // ONNX Runtime lit cette variable au moment de la création de session.
        let onnx_threads: usize = std::env::var("ONNX_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        // Injecter la limite dans l'environnement AVANT la création du modèle.
        // ort (ONNX Runtime Rust binding) respecte ORT_NUM_THREADS.
        // SAFETY: called once at init, before any ONNX sessions exist.
        unsafe { std::env::set_var("ORT_NUM_THREADS", onnx_threads.to_string()) };

        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::NomicEmbedTextV15)
                .with_show_download_progress(true),
        )
        .map_err(|e| {
            DomainError::Internal(format!(
                "Échec d'initialisation du modèle Nomic-Embed-Text-v1.5: {e}"
            ))
        })?;

        info!(
            model = "nomic-embed-text-v1.5",
            dimensions,
            onnx_threads,
            "🧬 EmbeddingService initialisé (ONNX Runtime local — {onnx_threads} threads)"
        );

        Ok(Self {
            model: Mutex::new(model),
            dimensions,
        })
    }

    /// Tronque un vecteur aux `dimensions` cibles et re-normalise (L2).
    ///
    /// C'est le cœur du Matryoshka Representation Learning :
    /// les premières N dimensions capturent l'essentiel de l'information.
    fn truncate_and_normalize(&self, mut vector: Vec<f32>) -> Vec<f32> {
        // Tronquer aux dimensions cibles.
        vector.truncate(self.dimensions);

        // Re-normaliser L2 pour que la similarité cosinus reste valide.
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vector {
                *v /= norm;
            }
        }

        vector
    }
}

impl std::fmt::Debug for NomicEmbedService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NomicEmbedService")
            .field("model", &"nomic-embed-text-v1.5")
            .field("dimensions", &self.dimensions)
            .finish()
    }
}

#[async_trait]
impl EmbeddingService for NomicEmbedService {
    #[instrument(skip(self, text), fields(text_len = text.len()))]
    async fn embed(&self, text: &str) -> Result<Vec<f32>, DomainError> {
        let text = text.to_string();
        let dimensions = self.dimensions;

        // fastembed n'est pas async — on exécute dans un thread bloquant.
        let model = self.model.lock().map_err(|e| {
            DomainError::Internal(format!("Mutex lock failed: {e}"))
        })?;

        let results = model.embed(vec![text], None).map_err(|e| {
            DomainError::Internal(format!("Embedding failed: {e}"))
        })?;

        drop(model); // Libérer le lock explicitement.

        let vector = results
            .into_iter()
            .next()
            .ok_or_else(|| DomainError::Internal("Empty embedding result".to_string()))?;

        // Tronquer et re-normaliser (Matryoshka).
        let mut truncated = vector;
        truncated.truncate(dimensions);
        let norm: f32 = truncated.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut truncated {
                *v /= norm;
            }
        }

        Ok(truncated)
    }

    #[instrument(skip(self, texts), fields(batch_size = texts.len()))]
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, DomainError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Phase 21 — Anti-OOM : Traiter en micro-batches de 32 pour limiter
        // l'empreinte mémoire ONNX. Sans cela, 159 chunks × 768d = explosion RAM.
        let batch_size: usize = std::env::var("ONNX_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32);

        let mut all_results: Vec<Vec<f32>> = Vec::with_capacity(texts.len());

        for (batch_idx, batch) in texts.chunks(batch_size).enumerate() {
            let batch_owned: Vec<String> = batch.to_vec();

            let model = self.model.lock().map_err(|e| {
                DomainError::Internal(format!("Mutex lock failed: {e}"))
            })?;

            let results = model.embed(batch_owned, None).map_err(|e| {
                DomainError::Internal(format!("Batch embedding failed: {e}"))
            })?;

            drop(model);

            // Tronquer et re-normaliser chaque vecteur.
            let truncated: Vec<Vec<f32>> = results
                .into_iter()
                .map(|v| self.truncate_and_normalize(v))
                .collect();

            info!(
                batch_idx,
                batch_len = truncated.len(),
                total_done = all_results.len() + truncated.len(),
                total = texts.len(),
                "🧬 Tensai — Micro-batch embeddings"
            );

            all_results.extend(truncated);
        }

        Ok(all_results)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}
