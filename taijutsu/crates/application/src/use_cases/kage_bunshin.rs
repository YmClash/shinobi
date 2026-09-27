//! Use Case : Kage Bunshin (影分身) — Phase 41 · Auto-Healing CI/CD 🥷⚡
//!
//! Orchestre la tentative de réparation automatique d'un stage CI/CD en échec.
//!
//! ## Flow
//! 1. Regex-parse les logs d'erreur → extraire les fichiers mentionnés
//! 2. Lire le contenu des fichiers ciblés depuis le VCS
//! 3. Appeler Sensei (LLM) avec le contexte limité
//! 4. Parser la réponse JSON (avec fallback regex)
//! 5. Créer le workspace shadow (jj workspace add)
//! 6. Appliquer les hunks (search/replace) avec normalisation \r\n
//! 7. Re-exécuter le stage dans le shadow (timeout 2min)
//! 8. Si succès → créer bookmark + MR automatique (auteur: Sensei service account)
//! 9. Nettoyer le shadow workspace (forget + remove_dir_all)
//!
//! ## Vegapunk Tweaks intégrés
//! - **#2** : Smart context — regex-parse stacktrace
//! - **#3** : `format: "json"` Ollama + regex fallback
//! - **#3b** : Hunks search/replace, pas de fichier complet
//! - **#5** : Timeout strict 2min sur shadow re-run
//! - **#8** : Normalisation `\r\n` → `\n` avant contains()
//! - **#9** : `remove_dir_all` après workspace_forget
//! - **#11** : Service Account Sensei comme auteur MR

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::kage_bunshin::{
    HealAttempt, HealResult, HealStatus, KageBunshinPatch, PatchHunk,
};
use domain::errors::DomainError;
use domain::ports::container_runner::ContainerRunner;
use domain::ports::llm_service::LlmService;
use domain::ports::pipeline_repository::PipelineRepository;

// ── Constants ────────────────────────────────────────────────────────

/// Nombre maximum de fichiers à envoyer comme contexte au LLM.
const MAX_CONTEXT_FILES: usize = 5;
/// Nombre maximum de hunks dans un patch.
const MAX_HUNKS: usize = 5;
/// Nombre maximum de lignes par hunk (search + replace).
const MAX_HUNK_LINES: usize = 20;
/// Score de confiance minimum pour appliquer un patch.
const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.5;

// ── KageBunshinUseCase ───────────────────────────────────────────────

/// Use case d'auto-healing CI/CD.
///
/// Orchestre : diagnostic Sensei → patch → shadow re-run → MR auto.
pub struct KageBunshinUseCase {
    llm: Arc<dyn LlmService>,
    pipeline_repo: Arc<dyn PipelineRepository>,
    container_runner: Arc<dyn ContainerRunner>,
    /// Timeout pour le shadow re-run (Vegapunk Tweak #5).
    shadow_timeout: Duration,
    /// Seuil de confiance minimum.
    confidence_threshold: f32,
}

impl KageBunshinUseCase {
    /// Construit le use case.
    pub fn new(
        llm: Arc<dyn LlmService>,
        pipeline_repo: Arc<dyn PipelineRepository>,
        container_runner: Arc<dyn ContainerRunner>,
        shadow_timeout: Duration,
        confidence_threshold: f32,
    ) -> Self {
        Self {
            llm,
            pipeline_repo,
            container_runner,
            shadow_timeout,
            confidence_threshold,
        }
    }

    /// Tentative de guérison d'un stage en échec.
    ///
    /// Appelé par le `KageBunshinConsumer` (file Kafka dédiée).
    ///
    /// # Arguments
    /// - `pipeline_id` : UUID du pipeline parent
    /// - `stage_id` : UUID du stage en échec
    /// - `stage_name` : nom du stage (ex: "Build")
    /// - `stage_image` : image Docker du stage
    /// - `stage_commands` : commandes du stage
    /// - `error_logs` : logs d'erreur du container
    /// - `repository_id` : UUID du dépôt
    /// - `commit_id` : SHA du commit
    /// - `workspace_path` : chemin du workspace éphémère contenant le code
    pub async fn attempt_heal(
        &self,
        pipeline_id: Uuid,
        _stage_id: Uuid,
        stage_name: &str,
        stage_image: &str,
        stage_commands: &[String],
        error_logs: &str,
        _repository_id: Uuid,
        _commit_id: &str,
        workspace_path: &Path,
    ) -> Result<HealResult, DomainError> {
        info!(
            pipeline_id = %pipeline_id,
            stage = %stage_name,
            "🥷 Kage Bunshin — analyse Sensei en cours..."
        );

        // ── 1. Extraire les fichiers mentionnés dans les logs ─────
        let error_files = extract_error_files(error_logs);
        info!(
            files = ?error_files,
            "📋 Fichiers extraits de la stacktrace ({} trouvés)",
            error_files.len()
        );

        // ── 2. Lire le contenu des fichiers ciblés ───────────────
        let source_context = self
            .collect_source_context(workspace_path, &error_files)
            .await;

        // ── 3. Appeler Sensei ────────────────────────────────────
        let prompt = build_sensei_prompt(
            stage_name,
            stage_image,
            stage_commands,
            error_logs,
            &source_context,
        );

        let system = build_system_prompt();

        let llm_response = self.llm.generate(&prompt, &system).await?;
        let llm_model = self.llm.model_name().to_string();
        let llm_duration_ms = llm_response.duration_ms as i32;

        info!(
            model = %llm_model,
            duration_ms = llm_duration_ms,
            "🧠 Sensei a répondu"
        );

        // ── 4. Parser la réponse JSON (avec fallback regex) ──────
        let patch = match parse_sensei_response(&llm_response.content) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    error = %e,
                    raw_len = llm_response.content.len(),
                    "⚠️ Sensei: réponse JSON invalide — abandon"
                );

                // Créer un heal_attempt en status failed
                let failed_patch = KageBunshinPatch {
                    diagnosis: format!("JSON parse error: {e}"),
                    patch_summary: String::new(),
                    hunks: vec![],
                    confidence: 0.0,
                };
                let attempt = HealAttempt::new(
                    pipeline_id,
                    stage_name,
                    &failed_patch,
                    Some(llm_model),
                    Some(llm_duration_ms),
                );
                let _ = self.pipeline_repo.create_heal_attempt(&attempt).await;

                return Ok(HealResult::Failed {
                    diagnosis: failed_patch.diagnosis,
                    reason: "Sensei returned invalid JSON".into(),
                });
            }
        };

        // ── 5. Vérifier le seuil de confiance ───────────────────
        if patch.confidence < self.confidence_threshold {
            info!(
                confidence = patch.confidence,
                threshold = self.confidence_threshold,
                "🚫 Confiance trop basse — patch ignoré"
            );

            let attempt = HealAttempt::new(
                pipeline_id,
                stage_name,
                &patch,
                Some(llm_model),
                Some(llm_duration_ms),
            );
            let attempt = self.pipeline_repo.create_heal_attempt(&attempt).await?;
            self.pipeline_repo
                .update_heal_attempt(
                    &attempt.id,
                    HealStatus::Failed,
                    None,
                    None,
                    Some("Confidence below threshold"),
                    None,
                )
                .await?;

            return Ok(HealResult::Failed {
                diagnosis: patch.diagnosis,
                reason: format!(
                    "Confidence {:.2} < threshold {:.2}",
                    patch.confidence, self.confidence_threshold
                ),
            });
        }

        // ── 6. Vérifier les garde-fous du patch ─────────────────
        if patch.hunks.is_empty() {
            let attempt = HealAttempt::new(
                pipeline_id,
                stage_name,
                &patch,
                Some(llm_model),
                Some(llm_duration_ms),
            );
            let attempt = self.pipeline_repo.create_heal_attempt(&attempt).await?;
            self.pipeline_repo
                .update_heal_attempt(
                    &attempt.id,
                    HealStatus::Failed,
                    None,
                    None,
                    Some("Empty hunks — Sensei has no fix to propose"),
                    None,
                )
                .await?;

            return Ok(HealResult::Failed {
                diagnosis: patch.diagnosis,
                reason: "No hunks in patch".into(),
            });
        }

        if patch.hunks.len() > MAX_HUNKS {
            return Ok(HealResult::Failed {
                diagnosis: patch.diagnosis,
                reason: format!("Too many hunks ({} > {})", patch.hunks.len(), MAX_HUNKS),
            });
        }

        // ── 7. Créer le heal_attempt en BDD ─────────────────────
        let attempt = HealAttempt::new(
            pipeline_id,
            stage_name,
            &patch,
            Some(llm_model.clone()),
            Some(llm_duration_ms),
        );
        let attempt = self.pipeline_repo.create_heal_attempt(&attempt).await?;

        // ── 8. Mettre à jour → healing ──────────────────────────
        self.pipeline_repo
            .update_heal_attempt(&attempt.id, HealStatus::Healing, None, None, None, None)
            .await?;

        // ── 9. Appliquer les hunks dans le workspace existant ────
        // Note: On applique directement dans le workspace éphémère (tempdir)
        // car le KageBunshinConsumer recrée un checkout frais pour le shadow.
        let applied = apply_hunks(workspace_path, &patch.hunks);
        if applied == 0 {
            warn!("⚠️ Aucun hunk appliqué — tous les search texts introuvables");
            self.pipeline_repo
                .update_heal_attempt(
                    &attempt.id,
                    HealStatus::Failed,
                    None,
                    None,
                    Some("No hunks could be applied — search text not found in files"),
                    None,
                )
                .await?;

            return Ok(HealResult::Failed {
                diagnosis: patch.diagnosis,
                reason: "No hunks applied — search text not found".into(),
            });
        }

        info!(
            applied = applied,
            total = patch.hunks.len(),
            "🔧 Hunks appliqués ({}/{})",
            applied,
            patch.hunks.len()
        );

        // ── 10. Re-exécuter le stage dans le workspace patché ────
        // Vegapunk Tweak #5 : timeout strict 2min
        let retry_result = self
            .container_runner
            .run_stage(
                stage_image,
                stage_commands,
                workspace_path,
                self.shadow_timeout,
                &pipeline_id.to_string(),
            )
            .await;

        match retry_result {
            Ok(result) => {
                let shadow_branch = format!(
                    "kage-bunshin/{}/{}",
                    stage_name.to_lowercase().replace(' ', "-"),
                    &pipeline_id.to_string()[..8]
                );

                if result.exit_code == 0 {
                    // ── SUCCESS ✅ ────────────────────────────────
                    info!(
                        stage = %stage_name,
                        shadow_branch = %shadow_branch,
                        "🥷✅ Kage Bunshin SUCCESS — shadow re-run passed"
                    );

                    self.pipeline_repo
                        .update_heal_attempt(
                            &attempt.id,
                            HealStatus::Success,
                            Some(&shadow_branch),
                            None, // MR sera rempli par le consumer après création
                            Some(&result.logs),
                            Some(result.exit_code as i16),
                        )
                        .await?;

                    Ok(HealResult::Success {
                        mr_id: Uuid::nil(), // Placeholder — le consumer créera la MR
                        diagnosis: patch.diagnosis,
                        shadow_branch,
                    })
                } else {
                    // ── FAILURE ❌ ────────────────────────────────
                    warn!(
                        stage = %stage_name,
                        exit_code = result.exit_code,
                        "🥷❌ Kage Bunshin FAILED — shadow re-run still fails"
                    );

                    self.pipeline_repo
                        .update_heal_attempt(
                            &attempt.id,
                            HealStatus::Failed,
                            Some(&shadow_branch),
                            None,
                            Some(&result.logs),
                            Some(result.exit_code as i16),
                        )
                        .await?;

                    Ok(HealResult::Failed {
                        diagnosis: patch.diagnosis,
                        reason: format!("Shadow re-run failed (exit_code={})", result.exit_code),
                    })
                }
            }
            Err(e) => {
                // Erreur infrastructure (Docker crash, timeout)
                warn!(
                    stage = %stage_name,
                    error = %e,
                    "🥷⚠️ Kage Bunshin ERROR — shadow re-run infrastructure error"
                );

                self.pipeline_repo
                    .update_heal_attempt(
                        &attempt.id,
                        HealStatus::Failed,
                        None,
                        None,
                        Some(&format!("Infrastructure error: {e}")),
                        Some(-1),
                    )
                    .await?;

                Ok(HealResult::Failed {
                    diagnosis: patch.diagnosis.clone(),
                    reason: format!("Shadow re-run error: {e}"),
                })
            }
        }
    }

    /// Collecte le contenu des fichiers mentionnés dans la stacktrace.
    ///
    /// Vegapunk Tweak #2 : ne pas envoyer tout le repo au LLM.
    /// Seulement les fichiers explicitement liés à l'erreur.
    async fn collect_source_context(
        &self,
        workspace_path: &Path,
        error_files: &[String],
    ) -> Vec<(String, String)> {
        let mut context = Vec::new();

        for file_path in error_files.iter().take(MAX_CONTEXT_FILES) {
            let full_path = workspace_path.join(file_path);
            match tokio::fs::read_to_string(&full_path).await {
                Ok(content) => {
                    // Cap à 200 lignes pour rester dans la fenêtre LLM
                    let lines: Vec<&str> = content.lines().collect();
                    let capped = if lines.len() > 200 {
                        format!(
                            "{}\n\n[... {} lignes supplémentaires omises ...]",
                            lines[..200].join("\n"),
                            lines.len() - 200
                        )
                    } else {
                        content
                    };
                    context.push((file_path.clone(), capped));
                }
                Err(e) => {
                    warn!(
                        path = %file_path,
                        error = %e,
                        "⚠️ Impossible de lire le fichier source pour le contexte"
                    );
                }
            }
        }

        context
    }
}

// ── Fonctions pures (testables) ──────────────────────────────────────

/// Extrait les chemins de fichiers mentionnés dans les logs d'erreur.
///
/// Parse les formats d'erreur courants :
/// - Rust : `--> src/main.rs:42:5`
/// - Node/TS : `at Object.<anonymous> (src/index.ts:15:3)`
/// - Python : `File "src/app.py", line 42`
/// - Go : `main.go:42:5`
///
/// Vegapunk Tweak #2 : smart context collection.
pub fn extract_error_files(error_logs: &str) -> Vec<String> {
    let patterns = [
        // Rust: --> src/main.rs:42:5
        r"-->\s+([a-zA-Z0-9_/\-\.]+\.rs):\d+",
        // Node/TS: (src/index.ts:15:3)
        r"\(([a-zA-Z0-9_/\-\.]+\.[tj]sx?):\d+",
        // Python: File "src/app.py", line 42
        r#"File\s+"([a-zA-Z0-9_/\-\.]+\.py)",\s+line"#,
        // Go: main.go:42:5
        r"([a-zA-Z0-9_/\-\.]+\.go):\d+:\d+",
        // C/C++: src/main.c:42:
        r"([a-zA-Z0-9_/\-\.]+\.[ch](?:pp)?):\d+:",
    ];

    let mut seen = HashSet::new();
    let mut files = Vec::new();

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            for cap in re.captures_iter(error_logs) {
                if let Some(m) = cap.get(1) {
                    let path = m.as_str().to_string();
                    // Ignorer les chemins système / dépendances
                    if !path.starts_with("/usr")
                        && !path.starts_with("/root")
                        && !path.contains("/registry/")
                        && !path.contains(".cargo/")
                        && !path.contains("node_modules/")
                        && seen.insert(path.clone())
                    {
                        files.push(path);
                    }
                }
            }
        }
    }

    // Limiter à MAX_CONTEXT_FILES
    files.truncate(MAX_CONTEXT_FILES);
    files
}

/// Construit le prompt utilisateur pour Sensei.
fn build_sensei_prompt(
    stage_name: &str,
    image: &str,
    commands: &[String],
    error_logs: &str,
    source_context: &[(String, String)],
) -> String {
    let mut prompt = format!(
        "STAGE: {stage_name}\nIMAGE: {image}\nCOMMANDS: {}\n\n",
        commands.join(" && ")
    );

    // Logs d'erreur (cap à 2000 chars pour le LLM)
    let error_cap = if error_logs.len() > 2000 {
        &error_logs[error_logs.len() - 2000..]
    } else {
        error_logs
    };
    prompt.push_str(&format!("ERROR LOGS:\n```\n{error_cap}\n```\n\n"));

    // Fichiers source
    for (path, content) in source_context {
        prompt.push_str(&format!("FILE: {path}\n```\n{content}\n```\n\n"));
    }

    prompt
}

/// Prompt système pour Sensei — spécialisé CI/CD auto-fix.
fn build_system_prompt() -> String {
    r#"Tu es un assistant de débogage CI/CD. Analyse le log d'erreur et propose
un correctif sous forme de hunks de remplacement.

RÉPONDS EN JSON STRICT :
{
  "diagnosis": "Description concise de l'erreur",
  "patch_summary": "Ce que le fix fait",
  "confidence": 0.0-1.0,
  "hunks": [
    {
      "path": "src/main.rs",
      "search": "texte_exact_à_chercher",
      "replace": "texte_de_remplacement"
    }
  ]
}

RÈGLES :
- "search" doit contenir du texte EXACT copié du fichier source.
- "replace" doit être le texte corrigé.
- Si la confiance < 0.3, renvoie "hunks": [].
- Ne modifie JAMAIS les fichiers de test.
- Max 5 hunks, max 20 lignes par hunk.
- Corrige UNIQUEMENT l'erreur visible dans les logs."#
        .to_string()
}

/// Parse la réponse Sensei avec fallback regex.
///
/// Vegapunk Tweak #3 : les petits LLM ajoutent souvent du texte
/// avant/après le JSON ("Voici le JSON demandé :"). On tente :
/// 1. JSON direct
/// 2. Extraction du bloc JSON via regex `\{.*\}`
pub fn parse_sensei_response(raw: &str) -> Result<KageBunshinPatch, DomainError> {
    // Tentative 1 : JSON direct
    if let Ok(patch) = serde_json::from_str::<KageBunshinPatch>(raw) {
        return Ok(patch);
    }

    // Tentative 2 : Extraire le premier bloc JSON complet
    // On cherche le pattern le plus greedy possible entre { et }
    let json_re = Regex::new(r"(?s)\{.*\}")
        .map_err(|e| DomainError::Internal(format!("regex error: {e}")))?;

    if let Some(m) = json_re.find(raw) {
        if let Ok(patch) = serde_json::from_str::<KageBunshinPatch>(m.as_str()) {
            return Ok(patch);
        }
    }

    Err(DomainError::Internal(format!(
        "Sensei: réponse JSON invalide (raw length: {} bytes)",
        raw.len()
    )))
}

/// Applique les hunks de remplacement sur les fichiers du workspace.
///
/// ## Vegapunk Tweak #8 — Normalisation \r\n
/// Avant de comparer avec `contains()`, normalise les retours à la ligne
/// dans le fichier ET dans le search text. Cela maximise les chances
/// de succès sur Windows et avec les LLM qui génèrent des `\r\n`.
///
/// ## Sécurité
/// Si le `search` text n'est pas trouvé dans le fichier, le hunk est
/// ignoré silencieusement (fail-safe). On retourne le nombre de hunks appliqués.
pub fn apply_hunks(workspace_path: &Path, hunks: &[PatchHunk]) -> u32 {
    let mut applied = 0;

    for hunk in hunks {
        let file_path = workspace_path.join(&hunk.path);

        // Lire le fichier
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    path = %hunk.path,
                    error = %e,
                    "⚠️ Hunk: impossible de lire le fichier — skipping"
                );
                continue;
            }
        };

        // Vegapunk Tweak #8 : normaliser les retours à la ligne
        let normalized_content = content.replace("\r\n", "\n");
        let normalized_search = hunk.search.replace("\r\n", "\n");
        let normalized_replace = hunk.replace.replace("\r\n", "\n");

        // Vérifier que le search text existe
        if !normalized_content.contains(&normalized_search) {
            warn!(
                path = %hunk.path,
                search_len = normalized_search.len(),
                "⚠️ Hunk search text not found — skipping"
            );
            continue;
        }

        // Vérifier la taille du hunk
        let search_lines = normalized_search.lines().count();
        let replace_lines = normalized_replace.lines().count();
        if search_lines > MAX_HUNK_LINES || replace_lines > MAX_HUNK_LINES {
            warn!(
                path = %hunk.path,
                search_lines,
                replace_lines,
                max = MAX_HUNK_LINES,
                "⚠️ Hunk trop gros — skipping"
            );
            continue;
        }

        // Appliquer le remplacement (une seule occurrence)
        let patched = normalized_content.replacen(&normalized_search, &normalized_replace, 1);

        // Écrire le fichier patchéd
        if let Err(e) = std::fs::write(&file_path, patched) {
            warn!(
                path = %hunk.path,
                error = %e,
                "⚠️ Hunk: impossible d'écrire le fichier patché"
            );
            continue;
        }

        info!(
            path = %hunk.path,
            search_lines,
            replace_lines,
            "✏️ Hunk appliqué"
        );
        applied += 1;
    }

    applied
}

// ── Tests unitaires ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_error_files_rust() {
        let logs = r#"
error[E0432]: unresolved import `uuid::Uuid`
 --> src/main.rs:5:5
  |
5 | use uuid::Uuid;
  |     ^^^^^^^^^^ no `Uuid` in the root

error[E0433]: failed to resolve
 --> src/lib.rs:42:9
  |
42|     let x = crate::Foo;
  |             ^^^^^^^^^^
        "#;

        let files = extract_error_files(logs);
        assert_eq!(files, vec!["src/main.rs", "src/lib.rs"]);
    }

    #[test]
    fn test_extract_error_files_dedup() {
        let logs = r#"
 --> src/main.rs:5:5
 --> src/main.rs:10:3
 --> src/lib.rs:42:9
        "#;

        let files = extract_error_files(logs);
        assert_eq!(files.len(), 2); // dedup
    }

    #[test]
    fn test_extract_error_files_ignores_system_paths() {
        let logs = r#"
 --> /usr/local/lib/rustlib/src/core/fmt/mod.rs:200:5
 --> /root/.cargo/registry/src/serde-1.0.0/src/de/mod.rs:100:5
 --> src/main.rs:5:5
        "#;

        let files = extract_error_files(logs);
        assert_eq!(files, vec!["src/main.rs"]);
    }

    #[test]
    fn test_extract_error_files_nodejs() {
        let logs = r#"
TypeError: Cannot read properties of undefined
    at Object.<anonymous> (src/index.ts:15:3)
    at processModule (src/utils/parser.tsx:42:10)
        "#;

        let files = extract_error_files(logs);
        assert!(files.contains(&"src/index.ts".to_string()));
        assert!(files.contains(&"src/utils/parser.tsx".to_string()));
    }

    #[test]
    fn test_parse_sensei_response_direct_json() {
        let json = r#"{"diagnosis":"missing import","patch_summary":"add use","confidence":0.8,"hunks":[{"path":"src/main.rs","search":"fn main()","replace":"use foo;\nfn main()"}]}"#;
        let patch = parse_sensei_response(json).unwrap();
        assert_eq!(patch.diagnosis, "missing import");
        assert_eq!(patch.hunks.len(), 1);
        assert!(patch.confidence > 0.7);
    }

    #[test]
    fn test_parse_sensei_response_with_markdown_wrapper() {
        let response = r#"Voici le JSON demandé :

```json
{"diagnosis":"typo in function","patch_summary":"fix typo","confidence":0.6,"hunks":[]}
```

J'espère que cela aide !"#;

        let patch = parse_sensei_response(response).unwrap();
        assert_eq!(patch.diagnosis, "typo in function");
    }

    #[test]
    fn test_parse_sensei_response_invalid() {
        let garbage = "This is not JSON at all, just plain text without any braces";
        assert!(parse_sensei_response(garbage).is_err());
    }

    #[test]
    fn test_apply_hunks_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("src/main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let hunks = vec![PatchHunk {
            path: "src/main.rs".into(),
            search: "fn main() {".into(),
            replace: "use uuid::Uuid;\n\nfn main() {".into(),
        }];

        let applied = apply_hunks(dir.path(), &hunks);
        assert_eq!(applied, 1);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("use uuid::Uuid;"));
        assert!(content.contains("fn main() {"));
    }

    #[test]
    fn test_apply_hunks_crlf_normalization() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("src/main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        // Fichier avec \r\n (Windows)
        std::fs::write(&file_path, "fn main() {\r\n    println!(\"hello\");\r\n}\r\n").unwrap();

        let hunks = vec![PatchHunk {
            path: "src/main.rs".into(),
            // LLM génère avec \n (Unix)
            search: "fn main() {\n    println!(\"hello\");".into(),
            replace: "fn main() {\n    println!(\"fixed\");".into(),
        }];

        // Vegapunk Tweak #8 : doit fonctionner malgré le mismatch \r\n vs \n
        let applied = apply_hunks(dir.path(), &hunks);
        assert_eq!(applied, 1);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("fixed"));
    }

    #[test]
    fn test_apply_hunks_search_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("src/main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let hunks = vec![PatchHunk {
            path: "src/main.rs".into(),
            search: "this text does not exist in the file".into(),
            replace: "replacement".into(),
        }];

        let applied = apply_hunks(dir.path(), &hunks);
        assert_eq!(applied, 0); // Aucun hunk appliqué
    }

    #[test]
    fn test_apply_hunks_file_not_found() {
        let dir = tempfile::TempDir::new().unwrap();

        let hunks = vec![PatchHunk {
            path: "nonexistent/file.rs".into(),
            search: "foo".into(),
            replace: "bar".into(),
        }];

        let applied = apply_hunks(dir.path(), &hunks);
        assert_eq!(applied, 0);
    }

    #[test]
    fn test_build_system_prompt_contains_json_format() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("JSON STRICT"));
        assert!(prompt.contains("diagnosis"));
        assert!(prompt.contains("hunks"));
    }
}
