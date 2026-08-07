//! Inbox Worker — Boucle de traitement des activités entrantes.
//!
//! Phase 32 — Le facteur qui trie le courrier.
//!
//! ## Architecture
//! - Boucle tokio polling toutes les 30s (ou immédiat si batch plein)
//! - FIFO séquentiel : respecte l'ordre causal des activités
//! - Batch de 20 activités par tick
//! - Shutdown gracieux via `CancellationToken`
//!
//! ## Poison Pill Protection
//! Si une activité est corrompue et fait planter le dispatch,
//! elle est TOUJOURS marquée `processed = true` avec un log ERROR.
//! Empêche une boucle infinie sur un message empoisonné.
//!
//! ## Scope V1
//! Observateur actif : log + classify + mark processed.
//! Les side-effects lourds (mirroring, sync) sont des points
//! d'extension pour les phases futures.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use domain::entities::federation::InboxActivity;
use domain::ports::federation_repository::FederationRepository;

/// Taille du batch d'activités par tick.
const BATCH_SIZE: i64 = 20;

/// Intervalle de polling quand l'inbox est vide.
const POLL_INTERVAL_SECS: u64 = 30;

/// Worker de traitement de l'inbox fédéré.
///
/// Tourne en boucle infinie dans un `tokio::spawn`.
/// Respecte le `CancellationToken` pour le shutdown gracieux.
pub struct InboxWorker {
    federation_repo: Arc<dyn FederationRepository>,
    cancel_token: CancellationToken,
}

impl InboxWorker {
    /// Construit le worker inbox.
    pub fn new(
        federation_repo: Arc<dyn FederationRepository>,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            federation_repo,
            cancel_token,
        }
    }

    /// Boucle principale du worker.
    ///
    /// ## Flow
    /// 1. Poll `list_unprocessed_inbox(BATCH_SIZE)`
    /// 2. Pour chaque activité : dispatch + mark processed
    /// 3. Si batch vide : sleep 30s
    /// 4. Si batch plein : boucle immédiatement (il y en a peut-être plus)
    /// 5. Si cancellation demandée : break
    pub async fn run(&self) {
        info!("📥 Inbox Worker — Démarrage de la boucle de traitement");

        loop {
            // ── Vérifier le shutdown ──
            if self.cancel_token.is_cancelled() {
                info!("📥 Inbox Worker — Arrêt demandé (CancellationToken)");
                break;
            }

            // ── Récupérer le batch d'activités non traitées ──
            let activities = match self.federation_repo.list_unprocessed_inbox(BATCH_SIZE).await {
                Ok(acts) => acts,
                Err(e) => {
                    error!(
                        error = %e,
                        "❌ Inbox Worker — Erreur de lecture des activités non traitées"
                    );
                    // Attendre avant de réessayer pour ne pas spammer les logs
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)) => {},
                        _ = self.cancel_token.cancelled() => {
                            info!("📥 Inbox Worker — Arrêt pendant le sleep d'erreur");
                            break;
                        }
                    }
                    continue;
                }
            };

            let batch_count = activities.len();

            if batch_count == 0 {
                // Rien à traiter — dormir
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)) => {},
                    _ = self.cancel_token.cancelled() => {
                        info!("📥 Inbox Worker — Arrêt pendant le sleep idle");
                        break;
                    }
                }
                continue;
            }

            info!(
                batch_size = batch_count,
                "📥 Inbox Worker — Traitement de {} activité(s)",
                batch_count
            );

            // ── Traiter chaque activité séquentiellement (FIFO) ──
            for activity in activities {
                self.process_activity(&activity).await;
            }

            // Si le batch était plein, il y en a peut-être plus → pas de sleep
            if batch_count < BATCH_SIZE as usize {
                // Petit délai pour ne pas boucler à vide
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                    _ = self.cancel_token.cancelled() => {
                        info!("📥 Inbox Worker — Arrêt entre les batches");
                        break;
                    }
                }
            }
        }

        info!("📥 Inbox Worker — Arrêté proprement");
    }

    /// Traite une activité individuelle.
    ///
    /// ## Poison Pill Protection
    /// Quoi qu'il arrive (succès ou erreur), l'activité est TOUJOURS
    /// marquée `processed = true`. Un message empoisonné ne doit
    /// jamais bloquer la file d'attente.
    async fn process_activity(&self, activity: &InboxActivity) {
        let activity_id = activity.id;
        let activity_type = &activity.activity_type;
        let remote_actor = &activity.remote_actor_uri;

        // ── Dispatch par type d'activité ──
        match self.dispatch(activity).await {
            Ok(()) => {
                info!(
                    activity_id = %activity_id,
                    activity_type = %activity_type,
                    remote_actor = %remote_actor,
                    "✅ Inbox Worker — Activité traitée"
                );
            }
            Err(e) => {
                // ⚠️ POISON PILL PROTECTION ⚠️
                // On logue l'erreur mais on marque quand même processed = true.
                // Un message corrompu ne doit JAMAIS bloquer le pipeline.
                error!(
                    activity_id = %activity_id,
                    activity_type = %activity_type,
                    remote_actor = %remote_actor,
                    error = %e,
                    "❌ Inbox Worker — Erreur de traitement (Poison Pill évité — marqué processed)"
                );
            }
        }

        // ── Toujours marquer comme traité ──
        if let Err(e) = self.federation_repo.mark_inbox_processed(&activity_id).await {
            error!(
                activity_id = %activity_id,
                error = %e,
                "❌ Inbox Worker — Impossible de marquer l'activité comme traitée"
            );
        }
    }

    /// Dispatch sémantique par type d'activité.
    ///
    /// Phase 32 V1 : Observateur actif (log + classify).
    /// Les side-effects lourds sont des points d'extension futurs.
    async fn dispatch(&self, activity: &InboxActivity) -> Result<(), String> {
        match activity.activity_type.as_str() {
            // ── Follow ────────────────────────────────────────
            // L'Accept a déjà été envoyé par inbox_handler (Phase 27-quater).
            // Le worker confirme juste le traitement.
            "Follow" => {
                self.handle_follow(activity).await
            }

            // ── Create ────────────────────────────────────────
            // Une forge distante a créé un dépôt.
            // V1: Log informatif. V2+: Mirror/federation sync.
            "Create" => {
                self.handle_create(activity).await
            }

            // ── Push ──────────────────────────────────────────
            // Une forge distante a poussé du code.
            // V1: Log informatif. V2+: Sync mirror branches.
            "Push" => {
                self.handle_push(activity).await
            }

            // ── Update ────────────────────────────────────────
            "Update" => {
                self.handle_update(activity).await
            }

            // ── Delete ────────────────────────────────────────
            "Delete" => {
                self.handle_delete(activity).await
            }

            // ── Announce (boost/share) ────────────────────────
            "Announce" => {
                self.handle_announce(activity).await
            }

            // ── Undo ──────────────────────────────────────────
            // Annulation d'un Follow ou autre activité.
            "Undo" => {
                self.handle_undo(activity).await
            }

            // ── Inconnu ───────────────────────────────────────
            unknown_type => {
                warn!(
                    activity_id = %activity.id,
                    activity_type = %unknown_type,
                    remote_actor = %activity.remote_actor_uri,
                    "⚠️ Inbox Worker — Type d'activité inconnu (skip)"
                );
                Ok(())
            }
        }
    }

    // ── Handlers par type ─────────────────────────────────────────

    async fn handle_follow(&self, activity: &InboxActivity) -> Result<(), String> {
        // L'Accept a déjà été envoyé par inbox_handler (réponse synchrone).
        // Le worker enregistre simplement le traitement différé.
        info!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            object_uri = %activity.object_uri,
            "🤝 Inbox Worker — Follow traité (Accept déjà envoyé par inbox_handler)"
        );
        Ok(())
    }

    async fn handle_create(&self, activity: &InboxActivity) -> Result<(), String> {
        let object_type = &activity.object_type;
        let object_uri = &activity.object_uri;

        info!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            object_type = %object_type,
            object_uri = %object_uri,
            "🏗️ Inbox Worker — Create {object_type} reçu (V1: log only, V2+: mirror)"
        );

        // Point d'extension V2 :
        // if object_type == "Repository" {
        //     self.mirror_service.init_remote_mirror(object_uri).await?;
        // }

        Ok(())
    }

    async fn handle_push(&self, activity: &InboxActivity) -> Result<(), String> {
        // Extraire les métadonnées de push du JSONB si disponibles
        let commits_count = activity.activity_json
            .get("object")
            .and_then(|o| o.get("totalItems"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        info!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            object_uri = %activity.object_uri,
            commits = commits_count,
            "📤 Inbox Worker — Push reçu ({commits_count} commits) (V1: log only, V2+: sync)"
        );

        // Point d'extension V2 :
        // self.sync_service.fetch_remote_changes(object_uri).await?;

        Ok(())
    }

    async fn handle_update(&self, activity: &InboxActivity) -> Result<(), String> {
        info!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            object_type = %activity.object_type,
            object_uri = %activity.object_uri,
            "✏️ Inbox Worker — Update reçu (V1: log only)"
        );
        Ok(())
    }

    async fn handle_delete(&self, activity: &InboxActivity) -> Result<(), String> {
        warn!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            object_type = %activity.object_type,
            object_uri = %activity.object_uri,
            "🗑️ Inbox Worker — Delete reçu (V1: log warning only)"
        );
        Ok(())
    }

    async fn handle_announce(&self, activity: &InboxActivity) -> Result<(), String> {
        info!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            object_uri = %activity.object_uri,
            "📢 Inbox Worker — Announce reçu (boost/share) (V1: log only)"
        );
        Ok(())
    }

    async fn handle_undo(&self, activity: &InboxActivity) -> Result<(), String> {
        // Extraire le type d'activité annulée
        let inner_type = activity.activity_json
            .get("object")
            .and_then(|o| o.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");

        info!(
            activity_id = %activity.id,
            remote_actor = %activity.remote_actor_uri,
            undo_target = %inner_type,
            "↩️ Inbox Worker — Undo {inner_type} reçu (V1: log only)"
        );

        // Point d'extension V2 :
        // if inner_type == "Follow" {
        //     self.federation_repo.delete_follow(
        //         &activity.remote_actor_uri,
        //         &activity.recipient_actor_id,
        //     ).await.map_err(|e| e.to_string())?;
        // }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    use domain::entities::federation::{
        FederationActivity, FederationFollow, FederationKeypair, InboxActivity,
    };
    use domain::errors::DomainError;

    // Mock repo qui retourne des activités non traitées
    struct MockInboxRepo {
        unprocessed: std::sync::Mutex<Vec<InboxActivity>>,
        processed_ids: std::sync::Mutex<Vec<Uuid>>,
    }

    impl MockInboxRepo {
        fn with_activities(activities: Vec<InboxActivity>) -> Self {
            Self {
                unprocessed: std::sync::Mutex::new(activities),
                processed_ids: std::sync::Mutex::new(vec![]),
            }
        }
    }

    #[async_trait::async_trait]
    impl FederationRepository for MockInboxRepo {
        async fn get_keypair(&self, _: &Uuid) -> Result<Option<FederationKeypair>, DomainError> { Ok(None) }
        async fn save_keypair(&self, _: &FederationKeypair) -> Result<(), DomainError> { Ok(()) }
        async fn save_follow(&self, _: &FederationFollow) -> Result<(), DomainError> { Ok(()) }
        async fn delete_follow(&self, _: &str, _: &Uuid) -> Result<bool, DomainError> { Ok(true) }
        async fn list_followers(&self, _: &Uuid) -> Result<Vec<FederationFollow>, DomainError> { Ok(vec![]) }
        async fn count_followers(&self, _: &Uuid) -> Result<i64, DomainError> { Ok(0) }
        async fn save_activity(&self, _: &FederationActivity) -> Result<(), DomainError> { Ok(()) }
        async fn list_activities(&self, _: &Uuid, _: i64) -> Result<Vec<FederationActivity>, DomainError> { Ok(vec![]) }
        async fn count_activities(&self, _: &Uuid) -> Result<i64, DomainError> { Ok(0) }
        async fn save_inbox_activity(&self, _: &InboxActivity) -> Result<(), DomainError> { Ok(()) }
        async fn list_inbox_activities(&self, _: &Uuid, _: i64) -> Result<Vec<InboxActivity>, DomainError> { Ok(vec![]) }
        async fn count_inbox_activities(&self, _: &Uuid) -> Result<i64, DomainError> { Ok(0) }
        async fn count_local_users(&self) -> Result<i64, DomainError> { Ok(0) }
        async fn count_local_repos(&self) -> Result<i64, DomainError> { Ok(0) }

        async fn mark_inbox_processed(&self, activity_id: &Uuid) -> Result<(), DomainError> {
            self.processed_ids.lock().unwrap().push(*activity_id);
            Ok(())
        }

        async fn list_unprocessed_inbox(&self, _limit: i64) -> Result<Vec<InboxActivity>, DomainError> {
            let mut lock = self.unprocessed.lock().unwrap();
            let result = lock.drain(..).collect();
            Ok(result)
        }
    }

    fn make_test_activity(activity_type: &str) -> InboxActivity {
        InboxActivity {
            id: Uuid::new_v4(),
            recipient_actor_id: Uuid::new_v4(),
            remote_actor_uri: "https://forgejo.example.com/users/alice".into(),
            activity_type: activity_type.into(),
            object_type: "Repository".into(),
            object_uri: "https://forgejo.example.com/repos/alice/my-lib".into(),
            activity_json: serde_json::json!({"type": activity_type}),
            processed: false,
            received_at: Utc::now(),
            processed_at: None,
        }
    }

    #[tokio::test]
    async fn test_worker_processes_and_marks_activities() {
        let activities = vec![
            make_test_activity("Create"),
            make_test_activity("Push"),
            make_test_activity("Follow"),
        ];
        let ids: Vec<Uuid> = activities.iter().map(|a| a.id).collect();

        let repo = Arc::new(MockInboxRepo::with_activities(activities));
        let cancel = CancellationToken::new();
        let worker = InboxWorker::new(repo.clone(), cancel.clone());

        // Lancer le worker et l'arrêter après un tick
        let worker_handle = tokio::spawn(async move {
            worker.run().await;
        });

        // Laisser le temps au worker de traiter
        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
        let _ = worker_handle.await;

        // Vérifier que toutes les activités ont été marquées processed
        let processed = repo.processed_ids.lock().unwrap();
        assert_eq!(processed.len(), 3, "Les 3 activités doivent être marquées processed");
        for id in &ids {
            assert!(processed.contains(id), "L'activité {id} doit être marquée processed");
        }
    }

    #[tokio::test]
    async fn test_worker_handles_unknown_activity_type() {
        let activities = vec![make_test_activity("SuperUnknownType")];
        let id = activities[0].id;

        let repo = Arc::new(MockInboxRepo::with_activities(activities));
        let cancel = CancellationToken::new();
        let worker = InboxWorker::new(repo.clone(), cancel.clone());

        let worker_handle = tokio::spawn(async move {
            worker.run().await;
        });

        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
        let _ = worker_handle.await;

        // Même un type inconnu doit être marqué processed (Poison Pill protection)
        let processed = repo.processed_ids.lock().unwrap();
        assert!(processed.contains(&id), "Les types inconnus doivent être marqués processed");
    }

    #[tokio::test]
    async fn test_worker_cancellation() {
        let repo = Arc::new(MockInboxRepo::with_activities(vec![]));
        let cancel = CancellationToken::new();
        let worker = InboxWorker::new(repo, cancel.clone());

        let worker_handle = tokio::spawn(async move {
            worker.run().await;
        });

        // Annuler immédiatement
        cancel.cancel();

        // Le worker doit s'arrêter proprement
        let result = tokio::time::timeout(Duration::from_secs(5), worker_handle).await;
        assert!(result.is_ok(), "Le worker doit s'arrêter dans les 5 secondes");
    }
}
