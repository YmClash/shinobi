//! Use Case : Exécution d'un Pipeline CI/CD — Phase 40 (Jutsu Runner) 🥷⚡
//!
//! Orchestrateur principal qui exécute un pipeline défini dans `jutsu.yml`
//! en pilotant des containers Docker via le port `ContainerRunner`.
//!
//! ## Dynamic Scheduling (V1 — Séquentiel)
//! Les stages sont exécutés séquentiellement. Le `requires` est respecté
//! en vérifiant que toutes les dépendances sont dans `completed_stages`
//! avant de lancer un stage. Si une dépendance a échoué, le stage est
//! marqué `skipped`.
//!
//! ## Workspace Éphémère (Vegapunk Tweak #1)
//! Le code source est matérialisé dans un `tempfile::TempDir` via
//! `git clone --shared` depuis le bare repo interne. Le TempDir est
//! automatiquement nettoyé quand il sort du scope (RAII Rust).
//!
//! ## Auto-report Commit Status (Phase 39 Integration)
//! Le pipeline reporte automatiquement son statut via l'API Commit Status
//! (context: `jutsu/<pipeline_name>`), ce qui affiche la pastille 🟢🔴🟡
//! à côté du commit dans l'UI Makimono.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::jutsu_config::JutsuConfig;
use domain::entities::pipeline::{
    Pipeline, PipelineStage, PipelineStageStatus, PipelineStatus, TriggerEvent,
};
use domain::errors::DomainError;
use domain::ports::container_runner::ContainerRunner;
use domain::ports::pipeline_repository::PipelineRepository;

use super::manage_commit_statuses::ManageCommitStatusesUseCase;

/// Use case d'exécution d'un pipeline CI/CD natif.
///
/// Orchestre l'exécution séquentielle des stages d'un `jutsu.yml`
/// dans des containers Docker éphémères.
pub struct RunPipelineUseCase {
    pipeline_repo: Arc<dyn PipelineRepository>,
    container_runner: Arc<dyn ContainerRunner>,
    commit_status_uc: Arc<ManageCommitStatusesUseCase>,
    stage_timeout: Duration,
    #[allow(dead_code)] // Sera utilisé pour le timeout global en V2
    pipeline_timeout: Duration,
}

impl RunPipelineUseCase {
    /// Construit le use case.
    pub fn new(
        pipeline_repo: Arc<dyn PipelineRepository>,
        container_runner: Arc<dyn ContainerRunner>,
        commit_status_uc: Arc<ManageCommitStatusesUseCase>,
        stage_timeout: Duration,
        pipeline_timeout: Duration,
    ) -> Self {
        Self {
            pipeline_repo,
            container_runner,
            commit_status_uc,
            stage_timeout,
            pipeline_timeout,
        }
    }

    /// Exécute un pipeline CI/CD complet.
    ///
    /// # Arguments
    /// - `owner` : handle du propriétaire du dépôt
    /// - `repo_name` : nom du dépôt
    /// - `repository_id` : UUID du dépôt
    /// - `commit_id` : SHA du commit Git à builder
    /// - `trigger_event` : événement déclencheur (push, mr_created, tag, manual)
    /// - `config` : configuration `jutsu.yml` déjà parsée et validée
    /// - `creator_id` : acteur ayant déclenché le pipeline (optionnel)
    /// - `workspace_path` : chemin du workspace éphémère (tempdir) contenant le code source
    pub async fn execute(
        &self,
        owner: &str,
        repo_name: &str,
        repository_id: Uuid,
        commit_id: &str,
        trigger_event: &str,
        config: JutsuConfig,
        creator_id: Option<Uuid>,
        workspace_path: &PathBuf,
    ) -> Result<Pipeline, DomainError> {
        let trigger = TriggerEvent::from_sql(trigger_event).unwrap_or(TriggerEvent::Manual);
        let context = format!("jutsu/{}", config.name);

        // ── 1. Créer le Pipeline en BDD (status: queued) ────────
        let pipeline = Pipeline::new(
            repository_id,
            commit_id.to_string(),
            trigger,
            Some(config.name.clone()),
            creator_id,
        );
        let pipeline = self.pipeline_repo.create(&pipeline).await?;
        let pipeline_id = pipeline.id;

        info!(
            pipeline_id = %pipeline_id,
            name = %config.name,
            stages = config.stages.len(),
            "🥷 Pipeline créé (status: queued)"
        );

        // ── 2. Créer les PipelineStages en BDD (status: pending) ──
        let mut stage_records = Vec::new();
        for (sort_order, (name, stage_def)) in config.stages.iter().enumerate() {
            let stage = PipelineStage::new(
                pipeline_id,
                name.clone(),
                stage_def.image.clone(),
                sort_order as i16,
            );
            let created = self.pipeline_repo.create_stage(&stage).await?;
            stage_records.push((name.clone(), created, stage_def.clone()));
        }

        // ── 3. Auto-report commit status → pending ──────────────
        if let Err(e) = self
            .commit_status_uc
            .create_or_update_status(
                creator_id.unwrap_or(Uuid::nil()),
                owner,
                repo_name,
                commit_id,
                "pending",
                &context,
                Some(format!("Pipeline '{}' démarré", config.name)),
                None,
            )
            .await
        {
            warn!(error = %e, "⚠️ Commit status report failed (non-fatal)");
        }

        // ── 4. Pipeline → running ───────────────────────────────
        let started_at = Utc::now();
        self.pipeline_repo
            .update_status(
                &pipeline_id,
                PipelineStatus::Running,
                Some(started_at),
                None,
                None,
            )
            .await?;

        // ── 5. Dynamic Scheduling (séquentiel V1) ───────────────
        let mut completed_stages: HashSet<String> = HashSet::new();
        let mut failed_stages: HashSet<String> = HashSet::new();
        let mut pipeline_failed = false;
        let mut pipeline_error = false;

        for (name, stage_record, stage_def) in &stage_records {
            // 5a. Vérifier les dépendances (requires)
            let deps_satisfied = stage_def
                .requires
                .iter()
                .all(|dep| completed_stages.contains(dep));

            let deps_failed = stage_def
                .requires
                .iter()
                .any(|dep| failed_stages.contains(dep));

            if deps_failed || (pipeline_failed && !deps_satisfied) {
                // Dépendance en échec → skip ce stage
                self.pipeline_repo
                    .update_stage_status(
                        &stage_record.id,
                        PipelineStageStatus::Skipped,
                        None,
                        Some(Utc::now()),
                        Some(0),
                    )
                    .await?;
                failed_stages.insert(name.clone());
                info!(stage = %name, "⏭️ Stage skipped (dépendance en échec)");
                continue;
            }

            // 5b. Stage → running
            let stage_start = Utc::now();
            self.pipeline_repo
                .update_stage_status(
                    &stage_record.id,
                    PipelineStageStatus::Running,
                    Some(stage_start),
                    None,
                    None,
                )
                .await?;

            info!(stage = %name, image = %stage_def.image, "🏃 Stage démarré");

            // 5c. Exécuter le container
            let run_result = self
                .container_runner
                .run_stage(
                    &stage_def.image,
                    &stage_def.jutsus,
                    workspace_path.as_path(),
                    self.stage_timeout,
                    &pipeline_id.to_string(),
                )
                .await;

            match run_result {
                Ok(result) => {
                    // 5d. Sauvegarder logs + exit_code
                    //
                    // ⚠️  Protection Fūinjutsu (Anti-saturation PostgreSQL) :
                    // Un build Rust/Node peut générer 10-50 Mo de logs par stage.
                    // PostgreSQL TEXT n'est pas conçu pour le streaming de logs bruts.
                    // Stratégie V1 : HEAD (100 lignes) + TAIL (200 lignes) + cap 64KB
                    //   → Les 100 premières lignes donnent le contexte de démarrage.
                    //   → Les 200 dernières lignes contiennent l'erreur finale (le plus utile).
                    // Stratégie V2 : stocker les logs bruts sur IPFS, sauvegarder le CID.
                    let logs_sanitized = sanitize_stage_logs(&result.logs);

                    self.pipeline_repo
                        .update_stage_logs(
                            &stage_record.id,
                            &logs_sanitized,
                            result.exit_code as i16,
                        )
                        .await?;

                    let stage_end = Utc::now();
                    let stage_duration = result.duration_ms as i32;

                    if result.exit_code == 0 {
                        // Success
                        self.pipeline_repo
                            .update_stage_status(
                                &stage_record.id,
                                PipelineStageStatus::Success,
                                None,
                                Some(stage_end),
                                Some(stage_duration),
                            )
                            .await?;
                        completed_stages.insert(name.clone());
                        info!(stage = %name, duration_ms = stage_duration, "✅ Stage réussi");
                    } else {
                        // Failure (exit_code ≠ 0)
                        self.pipeline_repo
                            .update_stage_status(
                                &stage_record.id,
                                PipelineStageStatus::Failure,
                                None,
                                Some(stage_end),
                                Some(stage_duration),
                            )
                            .await?;
                        failed_stages.insert(name.clone());
                        pipeline_failed = true;
                        warn!(
                            stage = %name,
                            exit_code = result.exit_code,
                            "❌ Stage échoué"
                        );
                    }
                }
                Err(e) => {
                    // Error infrastructure (Docker crash, timeout, etc.)
                    self.pipeline_repo
                        .update_stage_status(
                            &stage_record.id,
                            PipelineStageStatus::Error,
                            None,
                            Some(Utc::now()),
                            None,
                        )
                        .await?;
                    self.pipeline_repo
                        .update_stage_logs(
                            &stage_record.id,
                            &format!("[jutsu-runner] Infrastructure error: {e}"),
                            -1,
                        )
                        .await?;
                    failed_stages.insert(name.clone());
                    pipeline_failed = true;
                    pipeline_error = true;
                    warn!(
                        stage = %name,
                        error = %e,
                        "⚠️ Stage erreur infrastructure"
                    );
                }
            }
        }

        // ── 6. Calculer le statut final ─────────────────────────
        let finished_at = Utc::now();
        let duration_ms = (finished_at - started_at).num_milliseconds() as i32;

        let final_status = if pipeline_error {
            PipelineStatus::Error
        } else if pipeline_failed {
            PipelineStatus::Failure
        } else {
            PipelineStatus::Success
        };

        self.pipeline_repo
            .update_status(
                &pipeline_id,
                final_status.clone(),
                None,
                Some(finished_at),
                Some(duration_ms),
            )
            .await?;

        // ── 7. Auto-report commit status → résultat final ───────
        let status_state = match &final_status {
            PipelineStatus::Success => "success",
            PipelineStatus::Failure => "failure",
            _ => "error",
        };

        let status_desc = format!(
            "Pipeline '{}' {} en {}ms ({}/{} stages réussis)",
            config.name,
            final_status,
            duration_ms,
            completed_stages.len(),
            stage_records.len(),
        );

        if let Err(e) = self
            .commit_status_uc
            .create_or_update_status(
                creator_id.unwrap_or(Uuid::nil()),
                owner,
                repo_name,
                commit_id,
                status_state,
                &context,
                Some(status_desc),
                None,
            )
            .await
        {
            warn!(error = %e, "⚠️ Commit status final report failed (non-fatal)");
        }

        info!(
            pipeline_id = %pipeline_id,
            status = %final_status,
            duration_ms = duration_ms,
            completed = completed_stages.len(),
            failed = failed_stages.len(),
            "🏁 Pipeline terminé"
        );

        // ── 8. Retourner le pipeline mis à jour ─────────────────
        self.pipeline_repo
            .find_by_id(&pipeline_id)
            .await?
            .ok_or_else(|| DomainError::Internal("Pipeline just created but not found".to_string()))
    }
}

// ── Utilitaires ───────────────────────────────────────────────────────

/// Sanitise et tronque les logs d'un stage avant insertion en BDD.
///
/// ## Stratégie HEAD + TAIL (pattern CI/CD standard)
///
/// Plutôt qu'une troncature naïve par octet (qui coupe les lignes),
/// on garde les lignes les plus utiles :
/// - **HEAD** (100 lignes) : contexte de démarrage du container
///   (image pull, initialisation, premières erreurs)
/// - **TAIL** (200 lignes) : résultat final + trace d'erreur
///   (la zone la plus utile pour le debug)
///
/// Un séparateur explicite indique combien de lignes ont été omises.
///
/// ## Cap dur 64KB
/// Après la troncature ligne par ligne, un cap octet final à 64KB
/// protège contre les lignes pathologiquement longues (ex: base64 inline).
///
/// ## V2 — IPFS
/// Les logs bruts doivent être envoyés sur IPFS et seul le CID stocké en BDD.
/// Ce TODO sera adressé en Phase 41 (Jutsu IPFS Bridge).
///
/// ## Benchmarks de dimensionnement
/// - Une ligne de log typique Rust/Cargo : ~80-120 octets
/// - 300 lignes × 120 octets = ~36KB → bien sous la limite 64KB
/// - Un pipeline de 10 stages × 64KB = ~640KB total → acceptable en PG
pub fn sanitize_stage_logs(raw_logs: &str) -> String {
    const HEAD_LINES: usize = 100;
    const TAIL_LINES: usize = 200;
    const MAX_BYTES: usize = 64 * 1024; // 64KB cap dur

    let lines: Vec<&str> = raw_logs.lines().collect();
    let total = lines.len();

    // Cas simple : pas de troncature nécessaire
    if total <= HEAD_LINES + TAIL_LINES {
        return cap_bytes(raw_logs, MAX_BYTES);
    }

    // Troncature HEAD + TAIL
    let head: Vec<&str> = lines[..HEAD_LINES].to_vec();
    let tail: Vec<&str> = lines[total - TAIL_LINES..].to_vec();
    let omitted = total - HEAD_LINES - TAIL_LINES;

    let truncated = format!(
        "{}\n\n[... {} lignes omises — logs complets disponibles en V2 via IPFS ...]\n\n{}",
        head.join("\n"),
        omitted,
        tail.join("\n"),
    );

    cap_bytes(&truncated, MAX_BYTES)
}

/// Cap dur en octets — protège contre les lignes pathologiquement longues.
///
/// Cherche une coupure sur une frontière UTF-8 valide.
fn cap_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }

    // Reculer jusqu'à une frontière char valide
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }

    format!(
        "{}\n[cap 64KB atteint — {} octets total]",
        &s[..end],
        s.len()
    )
}

#[cfg(test)]
mod tests {
    use super::sanitize_stage_logs;

    #[test]
    fn test_short_logs_passthrough() {
        let logs = "line1\nline2\nline3\n";
        assert_eq!(sanitize_stage_logs(logs), logs);
    }

    #[test]
    fn test_truncation_preserves_head_and_tail() {
        // Générer 400 lignes
        let lines: Vec<String> = (0..400).map(|i| format!("line {i}")).collect();
        let raw = lines.join("\n");

        let result = sanitize_stage_logs(&raw);

        // Doit contenir la première ligne (HEAD)
        assert!(result.contains("line 0"));
        // Doit contenir la dernière ligne (TAIL)
        assert!(result.contains("line 399"));
        // Doit contenir le marqueur de troncature
        assert!(result.contains("lignes omises"));
        // Doit indiquer les 100 lignes omises (400 - 100 - 200 = 100)
        assert!(result.contains("100 lignes omises"));
    }

    #[test]
    fn test_64kb_hard_cap() {
        // Générer une seule ligne de 100KB
        let massive_line = "x".repeat(100 * 1024);
        let result = sanitize_stage_logs(&massive_line);
        assert!(result.len() <= 64 * 1024 + 100); // +100 pour le message de cap
    }

    #[test]
    fn test_exact_boundary() {
        // Exactement 300 lignes — pas de troncature
        let lines: Vec<String> = (0..300).map(|i| format!("line {i}")).collect();
        let raw = lines.join("\n");
        let result = sanitize_stage_logs(&raw);
        assert!(!result.contains("lignes omises"));
    }
}
