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
//!
//! ## Kage Bunshin V2 — Fail-Fast (Phase 42-pre-Fix)
//! Dès qu'un stage passe en `healing`, **tous les stages suivants sont
//! `skipped`** : un seul heal (donc une seule MR Sensei) par pipeline.
//! Le commit status passe en `pending` pendant la guérison, puis le
//! `KageBunshinConsumer` (ou le watchdog) finalise via
//! `FinalizePipelineUseCase`. Après le merge de la MR, le nouveau commit
//! déclenche un nouveau pipeline propre.

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
use domain::ports::event_publisher::EventPublisher;
use domain::ports::pipeline_repository::PipelineRepository;

use super::finalize_pipeline::{FinalizePipelineUseCase, FinalizedBy};

/// Use case d'exécution d'un pipeline CI/CD natif.
///
/// Orchestre l'exécution séquentielle des stages d'un `jutsu.yml`
/// dans des containers Docker éphémères.
pub struct RunPipelineUseCase {
    pipeline_repo: Arc<dyn PipelineRepository>,
    container_runner: Arc<dyn ContainerRunner>,
    /// Phase 42-pre-Fix — Finalisation unifiée (commit status + webhook).
    finalizer: Arc<FinalizePipelineUseCase>,
    /// Phase 41 — Publisher pour déléguer le heal à la file Kafka dédiée.
    event_publisher: Option<Arc<dyn EventPublisher>>,
    /// Phase 41 — Kage Bunshin activé globalement.
    kage_bunshin_enabled: bool,
    stage_timeout: Duration,
    #[allow(dead_code)] // Sera utilisé pour le timeout global en V2
    pipeline_timeout: Duration,
}

impl RunPipelineUseCase {
    /// Construit le use case.
    pub fn new(
        pipeline_repo: Arc<dyn PipelineRepository>,
        container_runner: Arc<dyn ContainerRunner>,
        finalizer: Arc<FinalizePipelineUseCase>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
        kage_bunshin_enabled: bool,
        stage_timeout: Duration,
        pipeline_timeout: Duration,
    ) -> Self {
        Self {
            pipeline_repo,
            container_runner,
            finalizer,
            event_publisher,
            kage_bunshin_enabled,
            stage_timeout,
            pipeline_timeout,
        }
    }

    /// Exécute un pipeline CI/CD complet.
    ///
    /// # Arguments
    /// - `repository_id` : UUID du dépôt (le commit status est reporté
    ///   directement par UUID — Phase 42-pre-Fix, plus de résolution owner/name)
    /// - `commit_id` : SHA du commit Git à builder
    /// - `trigger_event` : événement déclencheur (push, mr_created, tag, manual)
    /// - `config` : configuration `jutsu.yml` déjà parsée et validée
    /// - `creator_id` : acteur ayant déclenché le pipeline (optionnel)
    /// - `workspace_path` : chemin du workspace éphémère (tempdir) contenant le code source
    /// - `existing_pipeline_id` : si `Some`, reprend le pipeline déjà créé par le
    ///   trigger endpoint (évite le doublon « pipeline fantôme »). Si `None`,
    ///   crée un nouveau pipeline (cas du git push auto-trigger).
    ///
    /// # Errors
    /// - `DomainError::Duplicate` : un pipeline automatique existe déjà pour
    ///   ce (dépôt, commit, trigger) — index unique partiel (migration 032).
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        repository_id: Uuid,
        commit_id: &str,
        trigger_event: &str,
        config: JutsuConfig,
        creator_id: Option<Uuid>,
        workspace_path: &PathBuf,
        existing_pipeline_id: Option<Uuid>,
    ) -> Result<Pipeline, DomainError> {
        let trigger = TriggerEvent::from_sql(trigger_event).unwrap_or(TriggerEvent::Manual);

        // ── 1. Créer OU reprendre le Pipeline en BDD ────────────
        //
        // Phase 40-E-Fix : quand le trigger REST crée le pipeline
        // avant de publier sur Kafka, le JutsuConsumer reçoit l'ID
        // et le passe ici via `existing_pipeline_id`. On évite ainsi
        // la création d'un « pipeline fantôme » orphelin en BDD.
        let pipeline_record = if let Some(pid) = existing_pipeline_id {
            // Vérifier que le pipeline existe bien
            let mut existing = self.pipeline_repo.find_by_id(&pid).await?.ok_or_else(|| {
                DomainError::Internal(format!(
                    "existing_pipeline_id {pid} not found — race condition?"
                ))
            })?;

            // Phase 42-pre-Fix : le trigger manuel pré-crée le pipeline sans
            // nom (jutsu.yml pas encore lu). On le nomme AVANT le report
            // `pending`, sinon le contexte retombe sur `jutsu/pipeline`.
            if existing.pipeline_name.is_none() {
                self.pipeline_repo.set_pipeline_name(&pid, &config.name).await?;
                existing.pipeline_name = Some(config.name.clone());
            }

            info!(
                pipeline_id = %pid,
                name = %config.name,
                stages = config.stages.len(),
                "🥷 Pipeline existant repris (created by trigger)"
            );
            existing
        } else {
            // Nouveau pipeline (ex: git push auto-trigger, pas de pré-création)
            let pipeline = Pipeline::new(
                repository_id,
                commit_id.to_string(),
                trigger,
                Some(config.name.clone()),
                creator_id,
            );
            let pipeline = self.pipeline_repo.create(&pipeline).await?;

            info!(
                pipeline_id = %pipeline.id,
                name = %config.name,
                stages = config.stages.len(),
                "🥷 Pipeline créé (status: queued)"
            );
            pipeline
        };
        let pipeline_id = pipeline_record.id;

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
        self.finalizer
            .report_commit_status(
                &pipeline_record,
                "pending",
                format!("Pipeline '{}' démarré", config.name),
            )
            .await;

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
        let mut healing_stages: HashSet<String> = HashSet::new(); // VP-13: stages en cours de guérison
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

            // Phase 42-pre-Fix — Fail-Fast Kage Bunshin : un stage est en
            // guérison → on n'exécute plus rien (un seul heal par pipeline).
            if !healing_stages.is_empty() {
                self.pipeline_repo
                    .update_stage_status(
                        &stage_record.id,
                        PipelineStageStatus::Skipped,
                        None,
                        Some(Utc::now()),
                        Some(0),
                    )
                    .await?;
                info!(stage = %name, healing = ?healing_stages, "⏭️ Stage skipped (Fail-Fast — Kage Bunshin en cours)");
                continue;
            }

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
                    } else if stage_def.kage_bunshin && self.kage_bunshin_enabled && !pipeline_failed {
                        // ── Phase 41 — Kage Bunshin déclenché ──────────
                        // (Phase 42-pre-Fix : jamais si le pipeline a déjà
                        // échoué — Fail-Fast, le heal ne le sauverait pas.)
                        // Le stage a échoué mais kage_bunshin: true.
                        // On marque le stage comme Healing et on publie
                        // sur la file Kafka dédiée (Vegapunk Tweak #10).
                        // Le KageBunshinConsumer traitera le heal de manière
                        // asynchrone, sans bloquer les workers pipeline.
                        self.pipeline_repo
                            .update_stage_status(
                                &stage_record.id,
                                PipelineStageStatus::Healing,
                                None,
                                None, // pas encore terminé
                                None,
                            )
                            .await?;

                        // Sauvegarder les logs d'erreur pour le diagnostic
                        self.pipeline_repo
                            .update_stage_logs(
                                &stage_record.id,
                                &sanitize_stage_logs(&result.logs),
                                result.exit_code as i16,
                            )
                            .await?;

                        // Phase 42-pre-Fix — commit status `pending` AVANT la
                        // publication : le heal ne peut pas finir avant ce report.
                        self.finalizer
                            .report_commit_status(
                                &pipeline_record,
                                "pending",
                                format!("🥷 Kage Bunshin en cours sur '{name}'"),
                            )
                            .await;

                        // Publier sur la file Kafka dédiée
                        if let Some(publisher) = &self.event_publisher {
                            if let Err(e) = publisher
                                .publish_kage_bunshin_requested(
                                    pipeline_id,
                                    stage_record.id,
                                    name,
                                    &stage_def.image,
                                    &stage_def.jutsus,
                                    &result.logs,
                                    repository_id,
                                    commit_id,
                                )
                                .await
                            {
                                warn!(
                                    stage = %name,
                                    error = %e,
                                    "⚠️ Kage Bunshin: publication Kafka échouée — fallback failure"
                                );
                                // Fallback : marquer le stage comme failure
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
                            } else {
                                info!(
                                    stage = %name,
                                    "🥷 Kage Bunshin déclenché — heal en attente sur file dédiée"
                                );
                                // VP-13: Le stage est en Healing — le pipeline reste running.
                                // On NE marque PAS pipeline_failed = true ici.
                                // Le KageBunshinConsumer finalisera le statut du pipeline
                                // via finalize_pipeline_after_heal() une fois le heal terminé.
                                healing_stages.insert(name.clone());
                            }
                        } else {
                            // Pas de publisher — fallback failure
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
                        }
                    } else {
                        // Failure (exit_code ≠ 0, pas de kage_bunshin)
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
        let duration_ms = (Utc::now() - started_at).num_milliseconds() as i32;

        if !healing_stages.is_empty() && !pipeline_failed && !pipeline_error {
            // VP-13 + Phase 42-pre-Fix : un stage est en guérison.
            // Le pipeline reste "running" (aucune écriture ici : le
            // KageBunshinConsumer — ou le watchdog — finalisera via
            // FinalizePipelineUseCase, qui reporte commit status + webhook).
            info!(
                pipeline_id = %pipeline_id,
                healing = ?healing_stages,
                duration_ms,
                "🥷 Pipeline reste running — {} stage(s) en guérison (Fail-Fast)",
                healing_stages.len()
            );
        } else {
            let final_status = if pipeline_error {
                PipelineStatus::Error
            } else if pipeline_failed {
                PipelineStatus::Failure
            } else {
                PipelineStatus::Success
            };

            // ── 7. Finalisation unifiée (CAS + commit status + webhook) ──
            self.finalizer
                .complete(&pipeline_id, final_status.clone(), FinalizedBy::Runner)
                .await?;

            info!(
                pipeline_id = %pipeline_id,
                status = %final_status,
                duration_ms = duration_ms,
                completed = completed_stages.len(),
                failed = failed_stages.len(),
                "🏁 Pipeline terminé"
            );
        }

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

// ── Phase 42-pre-Fix — Tests d'orchestration (Fail-Fast Kage Bunshin) ──

#[cfg(test)]
mod orchestration_tests {
    use std::collections::{BTreeMap, HashMap};

    use domain::entities::jutsu_config::JutsuStage;
    use domain::entities::webhook::WebhookEventType;

    use super::*;
    use crate::use_cases::pipeline_test_support::{
        InMemoryPipelineRepo, RecordingPublisher, RecordingReporter, ScriptedRunner,
    };

    struct Harness {
        repo: Arc<InMemoryPipelineRepo>,
        reporter: Arc<RecordingReporter>,
        publisher: Arc<RecordingPublisher>,
        runner: Arc<ScriptedRunner>,
        uc: RunPipelineUseCase,
    }

    fn harness(exit_codes: &[(&str, i64)]) -> Harness {
        let repo = Arc::new(InMemoryPipelineRepo::default());
        let reporter = Arc::new(RecordingReporter::default());
        let publisher = Arc::new(RecordingPublisher::default());
        let runner = Arc::new(ScriptedRunner {
            exit_codes: exit_codes.iter().map(|(i, c)| (i.to_string(), *c)).collect::<HashMap<_, _>>(),
            ..Default::default()
        });
        let finalizer = Arc::new(FinalizePipelineUseCase::new(
            repo.clone(),
            reporter.clone(),
            Some(publisher.clone() as Arc<dyn EventPublisher>),
        ));
        let uc = RunPipelineUseCase::new(
            repo.clone(),
            runner.clone(),
            finalizer,
            Some(publisher.clone() as Arc<dyn EventPublisher>),
            true,
            Duration::from_secs(60),
            Duration::from_secs(600),
        );
        Harness { repo, reporter, publisher, runner, uc }
    }

    fn stage(image: &str, requires: &[&str], kage_bunshin: bool) -> JutsuStage {
        JutsuStage {
            image: image.to_string(),
            jutsus: vec!["cargo check".into()],
            requires: requires.iter().map(|s| s.to_string()).collect(),
            kage_bunshin,
        }
    }

    /// Scénario E2E kunai-calc : Futon_Check puis Suiton_Build (requires Futon_Check).
    fn e2e_config(build_requires_check: bool) -> JutsuConfig {
        let mut stages = BTreeMap::new();
        stages.insert("Futon_Check".to_string(), stage("check-img", &[], true));
        let req: &[&str] = if build_requires_check { &["Futon_Check"] } else { &[] };
        stages.insert("Suiton_Build".to_string(), stage("build-img", req, true));
        JutsuConfig { name: "E2E".into(), on: vec!["push".into()], stages }
    }

    async fn run(h: &Harness, config: JutsuConfig) -> Pipeline {
        h.uc.execute(Uuid::new_v4(), "f64e13775aab", "push", config, None, &PathBuf::from("."), None)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn healing_stage_skips_all_following_stages_single_heal() {
        // Même sans `requires` : Fail-Fast global dès qu'un heal démarre.
        let h = harness(&[("check-img", 101), ("build-img", 101)]);
        let pipeline = run(&h, e2e_config(false)).await;

        assert_eq!(*h.publisher.heals.lock().unwrap(), vec!["Futon_Check"]);
        assert_eq!(*h.runner.ran.lock().unwrap(), vec!["check-img"]);
        assert_eq!(h.repo.stage_by_name("Futon_Check").status, PipelineStageStatus::Healing);
        assert_eq!(h.repo.stage_by_name("Suiton_Build").status, PipelineStageStatus::Skipped);

        // Pipeline reste running, commit status pending, aucune annonce finale.
        assert_eq!(pipeline.status, PipelineStatus::Running);
        assert!(h.reporter.states().iter().all(|s| s == "pending"));
        assert!(h.publisher.webhooks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn real_exit_code_is_persisted_for_heal() {
        let h = harness(&[("check-img", 101)]);
        run(&h, e2e_config(true)).await;
        assert_eq!(h.repo.stage_by_name("Futon_Check").exit_code, Some(101));
    }

    #[tokio::test]
    async fn success_pipeline_is_finalized_and_announced() {
        let h = harness(&[]);
        let pipeline = run(&h, e2e_config(true)).await;

        assert_eq!(pipeline.status, PipelineStatus::Success);
        assert_eq!(h.reporter.states(), vec!["pending", "success"]);
        let webhooks = h.publisher.webhooks.lock().unwrap();
        assert_eq!(webhooks.len(), 1);
        assert_eq!(webhooks[0].event_type, WebhookEventType::PipelineCompleted);
        assert_eq!(webhooks[0].payload["finalized_by"], "runner");
    }

    #[tokio::test]
    async fn no_heal_when_pipeline_already_failed() {
        // Futon_Check sans kage_bunshin échoue ; Suiton_Build (indépendant,
        // kage_bunshin) échoue aussi → pas de heal, pipeline failure.
        let h = harness(&[("check-img", 1), ("build-img", 1)]);
        let mut config = e2e_config(false);
        config.stages.get_mut("Futon_Check").unwrap().kage_bunshin = false;

        let pipeline = run(&h, config).await;
        assert!(h.publisher.heals.lock().unwrap().is_empty());
        assert_eq!(pipeline.status, PipelineStatus::Failure);
        assert_eq!(h.reporter.states().last().unwrap(), "failure");
    }

    #[tokio::test]
    async fn manual_pipeline_without_name_is_named_from_config() {
        // Pipeline manuel pré-créé par le trigger REST : pipeline_name = None.
        let h = harness(&[]);
        let repo_id = Uuid::new_v4();
        let pre = Pipeline::new(repo_id, "f64e13775aab".into(), TriggerEvent::Manual, None, None);
        let pre = h.repo.create(&pre).await.unwrap();

        let pipeline = h
            .uc
            .execute(repo_id, "f64e13775aab", "manual", e2e_config(true), None, &PathBuf::from("."), Some(pre.id))
            .await
            .unwrap();

        assert_eq!(pipeline.pipeline_name.as_deref(), Some("E2E"));
        assert_eq!(h.repo.pipeline(&pre.id).pipeline_name.as_deref(), Some("E2E"));
        // Tous les commit statuses (pending + success) portent `jutsu/E2E`.
        let reports = h.reporter.reports.lock().unwrap();
        assert!(!reports.is_empty());
        assert!(reports.iter().all(|r| r.1 == "jutsu/E2E"), "{reports:?}");
    }
}
