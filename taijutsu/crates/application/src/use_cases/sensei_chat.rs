//! Use Case: SenseiChat — Agent conversationnel IA en temps réel.
//!
//! Sensei (先生) est le mentor IA de SHINOBI. Il orchestre la Triade :
//! - **Tensai** (RAG) : récupère les fragments de code pertinents
//! - **Oracle** (Reviews) : consulte les évaluations de qualité
//! - **LLM** (Ollama) : génère une réponse conversationnelle en streaming
//!
//! ## Flux
//! 1. L'utilisateur pose une question dans le SenseiPanel
//! 2. Sensei interroge Tensai (recherche sémantique pgvector)
//! 3. Sensei consulte la dernière review Oracle du commit courant
//! 4. Sensei assemble un Super-Prompt enrichi (fichier + RAG + Oracle + historique)
//! 5. Sensei streame la réponse token par token via SSE
//!
//! ## Architecture
//! Sensei est un agent **synchrone** (REST SSE), contrairement à Oracle et Tensai
//! qui sont des agents **asynchrones** (Kafka consumers). Cela garantit une
//! latence UI < 500ms (premier token).
//!
//! ## Phase 15 — L'Éveil du Mentor

use std::sync::Arc;

use tracing::{info, warn};

use domain::errors::DomainError;
use domain::ports::llm_service::{LlmService, LlmStreamChunk};
use domain::ports::repository::OperationRepository;
use domain::ports::review_repository::ReviewRepository;

use super::search_chunks::SearchChunksUseCase;

/// Use case: conversation IA contextuelle avec le développeur.
///
/// Sensei utilise un LLM conversationnel dédié (2ème instance Ollama)
/// pour répondre aux questions en s'appuyant sur le contexte du projet.
pub struct SenseiChatUseCase {
    /// RAG : interroge la mémoire sémantique de Tensai.
    search_chunks: Arc<SearchChunksUseCase>,
    /// Héritage Oracle : consulte les reviews et scores.
    review_repo: Arc<dyn ReviewRepository>,
    /// LLM conversationnel (Ollama #2, modèle Sensei).
    llm: Arc<dyn LlmService>,
    /// Contexte commit : pour résoudre la dernière opération (Phase 17+).
    _operation_repo: Arc<dyn OperationRepository>,
}

/// Message dans l'historique de conversation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" ou "assistant"
    pub content: String,
}

/// Requête de chat envoyée par le frontend.
#[derive(Debug)]
pub struct SenseiChatRequest {
    /// Question de l'utilisateur.
    pub query: String,
    /// Chemin du fichier ouvert dans l'explorateur.
    pub file_path: String,
    /// Contenu du fichier (tronqué à ~3000 chars côté frontend).
    pub file_content: Option<String>,
    /// Langage du fichier (ex: "rust", "typescript").
    pub language: Option<String>,
    /// Owner du dépôt (ex: "system").
    pub owner: String,
    /// Nom du dépôt (ex: "hello-world").
    pub repo: String,
    /// Historique de la conversation (multi-tour).
    pub history: Vec<ChatMessage>,
}

/// Métadonnées RAG et Oracle collectées avant l'appel LLM.
///
/// Sérialisé et envoyé au frontend comme événement SSE "context"
/// avant le début du streaming de tokens.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SenseiContext {
    /// Fragments RAG trouvés par Tensai.
    pub sources: Vec<SenseiSource>,
    /// Score Oracle du dernier commit (0-100), si disponible.
    pub oracle_score: Option<f32>,
    /// Résumé de la review Oracle, si disponible.
    pub oracle_summary: Option<String>,
}

/// Source RAG (fragment de code trouvé par Tensai).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SenseiSource {
    pub file_path: String,
    pub name: Option<String>,
    pub similarity: f32,
    pub language: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl SenseiChatUseCase {
    /// Construit le use case avec ses dépendances injectées.
    pub fn new(
        search_chunks: Arc<SearchChunksUseCase>,
        review_repo: Arc<dyn ReviewRepository>,
        llm: Arc<dyn LlmService>,
        operation_repo: Arc<dyn OperationRepository>,
    ) -> Self {
        Self {
            search_chunks,
            review_repo,
            llm,
            _operation_repo: operation_repo,
        }
    }

    /// Nom du modèle LLM actif (pour le badge UI et l'API /models).
    pub fn model_name(&self) -> &str {
        self.llm.model_name()
    }

    /// Exécute une conversation Sensei en streaming.
    ///
    /// ## Retourne
    /// Un tuple `(SenseiContext, Receiver<LlmStreamChunk>)` :
    /// - `SenseiContext` : métadonnées RAG + Oracle (envoyé comme 1er événement SSE)
    /// - `Receiver` : stream de tokens LLM
    pub async fn execute_stream(
        &self,
        request: SenseiChatRequest,
    ) -> Result<(SenseiContext, tokio::sync::mpsc::Receiver<LlmStreamChunk>), DomainError> {
        info!(
            query = %request.query,
            file_path = %request.file_path,
            owner = %request.owner,
            repo = %request.repo,
            history_len = request.history.len(),
            model = %self.llm.model_name(),
            "🥷 Sensei — Nouvelle question reçue"
        );

        // ── 1. RAG : Interroger Tensai (recherche sémantique) ──────────
        let rag_chunks = match self
            .search_chunks
            .execute_semantic(&request.query, 5, 0.3)
            .await
        {
            Ok(result) => {
                info!(
                    count = result.count,
                    "🥷 Sensei — {} fragments RAG trouvés via Tensai", result.count
                );
                result.chunks
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "⚠️ Sensei — Recherche RAG échouée (graceful degradation)"
                );
                Vec::new()
            }
        };

        // Construire les sources pour le frontend.
        let sources: Vec<SenseiSource> = rag_chunks
            .iter()
            .map(|c| SenseiSource {
                file_path: c.chunk.file_path.clone(),
                name: c.chunk.name.clone(),
                similarity: c.similarity,
                language: c.chunk.language.clone(),
                start_line: c.chunk.start_line,
                end_line: c.chunk.end_line,
            })
            .collect();

        // ── 2. Oracle : Consulter la dernière review ──────────────────
        let (oracle_score, oracle_summary) = self.fetch_oracle_context().await;

        // Construire le contexte à envoyer au frontend.
        let context = SenseiContext {
            sources: sources.clone(),
            oracle_score,
            oracle_summary: oracle_summary.clone(),
        };

        // ── 3. Assembler le Super-Prompt ──────────────────────────────
        let prompt = build_sensei_prompt(
            &request.query,
            &request.file_path,
            request.file_content.as_deref(),
            request.language.as_deref(),
            &rag_chunks,
            oracle_score,
            oracle_summary.as_deref(),
            &request.history,
        );

        info!(
            prompt_len = prompt.len(),
            sources = sources.len(),
            oracle_score = ?oracle_score,
            "🥷 Sensei — Super-Prompt assemblé"
        );

        // ── 4. Streaming LLM ─────────────────────────────────────────
        let rx = self
            .llm
            .generate_stream(&prompt, SENSEI_SYSTEM_PROMPT)
            .await?;

        Ok((context, rx))
    }

    /// Récupère le contexte Oracle (dernière review avec score).
    async fn fetch_oracle_context(&self) -> (Option<f32>, Option<String>) {
        // Récupérer les derniers scores pour trouver la review la plus récente.
        match self.review_repo.find_recent_scores(1).await {
            Ok(scores) if !scores.is_empty() => {
                let score = scores[0].score;
                let op_id = scores[0].operation_id;

                // Récupérer la review complète pour le résumé.
                match self.review_repo.find_by_operation(&op_id).await {
                    Ok(reviews) if !reviews.is_empty() => {
                        let review = &reviews[0];
                        (
                            Some(score * 100.0), // Convertir 0.0-1.0 → 0-100
                            Some(review.summary.clone()),
                        )
                    }
                    _ => (Some(score * 100.0), None),
                }
            }
            Ok(_) => {
                info!("🥷 Sensei — Aucune review Oracle disponible");
                (None, None)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "⚠️ Sensei — Récupération Oracle échouée (graceful degradation)"
                );
                (None, None)
            }
        }
    }
}

// ── System Prompt Sensei ──────────────────────────────────────────────

/// Prompt système du mentor Sensei (先生).
///
/// Définit la personnalité, les règles, et le format de sortie.
/// Contrairement à l'Oracle (strict, scoring), Sensei est pédagogue
/// et conversationnel.
const SENSEI_SYSTEM_PROMPT: &str = r#"Tu es Sensei , le mentor IA du projet SHINOBI.
Tu guides les développeurs avec pédagogie, expertise et bienveillance.

Tu as accès à trois sources de contexte :
1. **Le fichier source** ouvert par le développeur dans l'explorateur de code
2. **Les fragments RAG** : extraits de code similaires trouvés par Tensai (recherche sémantique vectorielle)
3. **L'évaluation Oracle** : le score qualité (0-100) et la critique technique du dernier commit

Règles :
- Réponds en français, en Markdown structuré
- Sois pédagogue : explique le "pourquoi" autant que le "comment"
- Cite les fragments de code pertinents avec le nom du fichier source
- Si l'Oracle a détecté des problèmes, mentionne-les en les contextualisant
- Utilise des emojis pour structurer (✅ ⚠️ 💡 🐛 📝)
- Sois concis mais complet
- Si tu ne sais pas, dis-le honnêtement
- N'invente pas de code : base-toi uniquement sur le contexte fourni"#;

// ── Construction du Super-Prompt ──────────────────────────────────────

/// Construit le prompt utilisateur enrichi avec tout le contexte Sensei.
///
/// Assemblage : fichier ouvert + fragments RAG + review Oracle + historique + question.
fn build_sensei_prompt(
    query: &str,
    file_path: &str,
    file_content: Option<&str>,
    language: Option<&str>,
    rag_chunks: &[domain::ports::chunk_repository::SimilarChunk],
    oracle_score: Option<f32>,
    oracle_summary: Option<&str>,
    history: &[ChatMessage],
) -> String {
    let mut prompt = String::with_capacity(8192);

    // ── Section fichier ouvert ───────────────
    let lang = language.unwrap_or("text");
    prompt.push_str(&format!("## Fichier ouvert : `{file_path}` ({lang})\n\n"));

    if let Some(content) = file_content {
        // Tronquer à ~3000 caractères pour rester dans la fenêtre de contexte.
        let truncated = if content.len() > 3000 {
            format!(
                "{}...\n[tronqué — {} octets au total]",
                &content[..3000],
                content.len()
            )
        } else {
            content.to_string()
        };
        prompt.push_str(&format!("```{lang}\n{truncated}\n```\n\n"));
    }

    // ── Section RAG (fragments Tensai) ───────
    if !rag_chunks.is_empty() {
        prompt.push_str("## Fragments de code pertinents (trouvés par Tensai)\n\n");
        for (i, similar) in rag_chunks.iter().enumerate() {
            let chunk = &similar.chunk;
            let name = chunk.name.as_deref().unwrap_or("(anonyme)");
            let similarity_pct = (similar.similarity * 100.0) as u32;
            prompt.push_str(&format!(
                "### {}. `{}` — {} (L{}-{}, similarité: {}%)\n",
                i + 1,
                chunk.file_path,
                name,
                chunk.start_line,
                chunk.end_line,
                similarity_pct,
            ));
            // Limiter chaque chunk à 500 chars pour ne pas exploser le contexte.
            let chunk_content = if chunk.content.len() > 500 {
                format!("{}...", &chunk.content[..500])
            } else {
                chunk.content.clone()
            };
            prompt.push_str(&format!(
                "```{}\n{}\n```\n\n",
                chunk.language, chunk_content
            ));
        }
    }

    // ── Section Oracle (review + score) ──────
    if oracle_score.is_some() || oracle_summary.is_some() {
        prompt.push_str("## Évaluation de l'Oracle\n\n");
        if let Some(score) = oracle_score {
            prompt.push_str(&format!("**Score qualité** : {:.0}/100\n", score));
        }
        if let Some(summary) = oracle_summary {
            prompt.push_str(&format!("**Résumé** : {summary}\n"));
        }
        prompt.push('\n');
    }

    // ── Historique de conversation ────────────
    if !history.is_empty() {
        prompt.push_str("## Historique de la conversation\n\n");
        for msg in history {
            let role_label = if msg.role == "user" {
                "Développeur"
            } else {
                "Sensei"
            };
            prompt.push_str(&format!("**{role_label}** : {}\n\n", msg.content));
        }
    }

    // ── Question de l'utilisateur ─────────────
    prompt.push_str("## Question du développeur\n\n");
    prompt.push_str(query);
    prompt.push('\n');

    prompt
}
