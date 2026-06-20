//! Use Case: ReviewOperation — Code review automatique par l'Oracle.
//!
//! C'est le cœur de l'agent actif. Ce use case reçoit un signal
//! `AnalysisCompleteSummary` depuis Kafka, récupère le diff et le code
//! source du commit, les soumet à un LLM local (Ollama) pour obtenir
//! une code review, et persiste le résultat.
//!
//! ## Pipeline
//! 1. Réception de `AnalysisCompleteSummary` (depuis Kafka `analysis-complete`)
//! 2. Récupération de l'opération (OperationRepository)
//! 3. Récupération du diff VCS (VcsEngine)
//! 4. Récupération du code source depuis IPFS (ContentStore)
//! 5. Construction du prompt (system + user)
//! 6. Appel au LLM (Ollama via LlmService)
//! 7. Idempotence : delete_by_operation (ReviewRepository)
//! 8. Persistence de la review (ReviewRepository)

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;
use domain::ports::llm_service::LlmService;
use domain::ports::repository::OperationRepository;
use domain::ports::review_repository::{OperationReview, ReviewRepository};
use domain::ports::vcs_engine::VcsEngine;

/// Use case: produire une code review IA pour une opération VCS.
///
/// L'Oracle utilise un LLM local (Ollama) pour analyser le diff
/// d'un commit et produire des commentaires constructifs.
pub struct ReviewOperationUseCase {
    repository: Arc<dyn OperationRepository>,
    vcs: Arc<dyn VcsEngine>,
    llm: Arc<dyn LlmService>,
    review_repo: Arc<dyn ReviewRepository>,
    content_store: Arc<dyn ContentStore>,
}

/// Résultat de la review : soit une review complète, soit un skip motivé.
#[derive(Debug)]
pub enum ReviewOutcome {
    /// Review produite avec succès.
    Reviewed {
        review_id: Uuid,
        operation_id: Uuid,
        duration_ms: u64,
    },
    /// Opération ignorée (pas de diff, pas de code source, etc.).
    Skipped {
        operation_id: Uuid,
        reason: String,
    },
}

impl ReviewOperationUseCase {
    /// Construit le use case avec ses dépendances injectées.
    pub fn new(
        repository: Arc<dyn OperationRepository>,
        vcs: Arc<dyn VcsEngine>,
        llm: Arc<dyn LlmService>,
        review_repo: Arc<dyn ReviewRepository>,
        content_store: Arc<dyn ContentStore>,
    ) -> Self {
        Self {
            repository,
            vcs,
            llm,
            review_repo,
            content_store,
        }
    }

    /// Exécute la code review d'une opération.
    ///
    /// ## Flux
    /// 1. Retrouve l'opération → skip si absente
    /// 2. Récupère le diff VCS → skip si aucun fichier modifié
    /// 3. Récupère le code source depuis IPFS → skip si absent
    /// 4. Construit le prompt → appelle le LLM
    /// 5. Persiste la review (idempotent via delete + save)
    pub async fn execute(&self, operation_id: Uuid) -> Result<ReviewOutcome, DomainError> {
        let start = Instant::now();

        // 1. Retrouver l'opération.
        let operation = match self.repository.find_by_id(&operation_id).await? {
            Some(op) => op,
            None => {
                info!(
                    operation_id = %operation_id,
                    "🔮 Oracle — Skip: Opération introuvable"
                );
                return Ok(ReviewOutcome::Skipped {
                    operation_id,
                    reason: "Opération introuvable".to_string(),
                });
            }
        };

        // 2. Récupérer le diff VCS (optionnel pour les opérations racine).
        let content_id = ContentId::new(operation.content_id.clone().into_inner());
        let changed_files = if !operation.parent_ids.is_empty() {
            let parent_op = self.repository.find_by_id(&operation.parent_ids[0]).await?;
            match parent_op {
                Some(parent) => {
                    let parent_cid = ContentId::new(parent.content_id.into_inner());
                    self.vcs.diff_since(&parent_cid).await.unwrap_or_default()
                }
                None => self.vcs.diff_since(&content_id).await.unwrap_or_default(),
            }
        } else {
            // Opération racine (pas de parent) — on tente quand même un diff.
            // Si le diff est vide, on procédera avec le contenu IPFS seul.
            self.vcs.diff_since(&content_id).await.unwrap_or_default()
        };

        // Pour les opérations racine sans diff, on continue si IPFS est disponible.
        // Le LLM peut reviewer le code source même sans diff explicite.
        let is_root = operation.parent_ids.is_empty();
        if changed_files.is_empty() && !is_root {
            info!(
                operation_id = %operation_id,
                "🔮 Oracle — Skip: Aucun fichier modifié détecté"
            );
            return Ok(ReviewOutcome::Skipped {
                operation_id,
                reason: "Aucun fichier modifié détecté".to_string(),
            });
        }

        // 3. Récupérer le code source depuis IPFS.
        let source_files = match &operation.ipfs_content_id {
            Some(ipfs_cid) => {
                match self.content_store.retrieve(ipfs_cid).await {
                    Ok(blob) => {
                        // Décoder le blob JSON → fichiers.
                        match serde_json::from_slice::<Vec<IpfsFileEntry>>(&blob) {
                            Ok(files) => files,
                            Err(e) => {
                                warn!(
                                    operation_id = %operation_id,
                                    error = %e,
                                    "⚠️ Oracle — Décodage blob IPFS échoué"
                                );
                                Vec::new()
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            operation_id = %operation_id,
                            error = %e,
                            "⚠️ Oracle — Récupération IPFS échouée"
                        );
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };

        // 4. Construire le prompt.
        let prompt = build_prompt(
            &operation.description,
            &changed_files,
            &source_files,
        );

        let system = SYSTEM_PROMPT;

        // 5. Appeler le LLM.
        info!(
            operation_id = %operation_id,
            model = %self.llm.model_name(),
            prompt_len = prompt.len(),
            changed_files = changed_files.len(),
            "🔮 Oracle — Envoi au LLM pour code review..."
        );

        let llm_response = self.llm.generate(&prompt, system).await?;

        // 6. Construire la review.
        let summary = extract_summary(&llm_response.content);
        let review = OperationReview {
            id: Uuid::new_v4(),
            operation_id,
            reviewer: "oracle".to_string(),
            model: llm_response.model.clone(),
            summary,
            content: llm_response.content.clone(),
            score: None, // Le score pourrait être extrait du LLM dans une version future.
            duration_ms: llm_response.duration_ms,
            created_at: Utc::now(),
        };

        // 7. Idempotence : supprimer les reviews existantes.
        match self.review_repo.delete_by_operation(&operation_id).await {
            Ok(deleted) if deleted > 0 => {
                info!(
                    operation_id = %operation_id,
                    deleted_reviews = deleted,
                    "🗑️ Oracle — Reviews précédentes supprimées (idempotence)"
                );
            }
            Ok(_) => {} // Première review — rien à supprimer.
            Err(e) => {
                warn!(
                    operation_id = %operation_id,
                    error = %e,
                    "⚠️ Oracle — Erreur delete_by_operation reviews (non-fatal)"
                );
            }
        }

        // 8. Persister la review.
        self.review_repo.save_review(&review).await?;

        let total_duration_ms = start.elapsed().as_millis() as u64;

        info!(
            operation_id = %operation_id,
            review_id = %review.id,
            model = %review.model,
            llm_duration_ms = review.duration_ms,
            total_duration_ms,
            content_len = review.content.len(),
            "🔮 Oracle — Review persistée avec succès"
        );

        Ok(ReviewOutcome::Reviewed {
            review_id: review.id,
            operation_id,
            duration_ms: total_duration_ms,
        })
    }
}

// ── Types internes ────────────────────────────────────────────────────

/// Entrée de fichier dans le blob JSON IPFS.
#[derive(Debug, serde::Deserialize)]
struct IpfsFileEntry {
    path: String,
    content_b64: String,
    #[allow(dead_code)]
    size: usize,
}

// ── Prompts ───────────────────────────────────────────────────────────

/// Prompt système pour l'Oracle Reviewer.
const SYSTEM_PROMPT: &str = r#"Tu es un développeur senior bienveillant et expérimenté qui fait des code reviews.

Règles :
- Sois concis et constructif. Pas de flatterie vide.
- Utilise le format Markdown avec des emojis pour structurer ta review.
- Identifie les points suivants :
  1. ✅ **Points forts** — Ce qui est bien fait
  2. ⚠️ **Suggestions** — Améliorations possibles
  3. 🐛 **Bugs potentiels** — Problèmes détectés
  4. 💡 **Recommandations** — Bonnes pratiques à appliquer
- Si le code est bon, dis-le simplement. Pas besoin d'inventer des problèmes.
- Réponds en français.
- Commence ta réponse par un résumé d'une seule phrase."#;

/// Construit le prompt utilisateur à partir du contexte de l'opération.
fn build_prompt(
    description: &str,
    changed_files: &[String],
    source_files: &[IpfsFileEntry],
) -> String {
    let mut prompt = String::with_capacity(4096);

    // Section contexte.
    prompt.push_str("## Contexte du Commit\n\n");
    prompt.push_str(&format!("**Description** : {}\n\n", description));
    prompt.push_str(&format!(
        "**Fichiers modifiés** ({}) :\n",
        changed_files.len()
    ));
    for file in changed_files {
        prompt.push_str(&format!("- `{}`\n", file));
    }
    prompt.push('\n');

    // Section code source (depuis IPFS, décodé du base64).
    if !source_files.is_empty() {
        prompt.push_str("## Code Source\n\n");

        for file in source_files {
            // Détecter le langage pour le syntax highlighting dans le prompt.
            let lang = detect_language(&file.path).unwrap_or("text");

            // Décoder le contenu base64.
            let content = match base64_decode(&file.content_b64) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                Err(_) => continue,
            };

            // Limiter la taille du code pour rester dans les limites du contexte LLM.
            let truncated = if content.len() > 3000 {
                format!("{}...\n[tronqué — {} octets au total]", &content[..3000], content.len())
            } else {
                content
            };

            prompt.push_str(&format!("### `{}`\n", file.path));
            prompt.push_str(&format!("```{}\n{}\n```\n\n", lang, truncated));
        }
    }

    prompt.push_str("## Instructions\n\n");
    prompt.push_str("Fais une code review concise et bienveillante du code ci-dessus.\n");

    prompt
}

/// Extrait la première ligne significative comme résumé.
fn extract_summary(content: &str) -> String {
    content
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .unwrap_or("Review générée par l'Oracle")
        .trim()
        .chars()
        .take(200)
        .collect()
}

/// Détecte le langage à partir de l'extension du fichier.
fn detect_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "css" => Some("css"),
        "py" => Some("python"),
        "js" => Some("javascript"),
        "json" => Some("json"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "md" => Some("markdown"),
        "sql" => Some("sql"),
        "html" => Some("html"),
        _ => None,
    }
}

// ── Base64 décodeur RFC 4648 ─────────────────────────────────────────

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for ch in input.chars() {
        let val = match ch {
            'A'..='Z' => (ch as u32) - ('A' as u32),
            'a'..='z' => (ch as u32) - ('a' as u32) + 26,
            '0'..='9' => (ch as u32) - ('0' as u32) + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("Caractère base64 invalide: {ch}")),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buf >> bits) & 0xFF) as u8);
        }
    }

    Ok(output)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "review_operation_test.rs"]
mod tests;
