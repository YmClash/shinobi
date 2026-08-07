//! Fanout Service — Outbox + Livraison signée aux followers.
//!
//! Phase 27-ter — Orchestre la publication d'activités fédérées :
//! 1. Sauvegarde dans l'outbox PostgreSQL
//! 2. Liste les followers fédérés
//! 3. Livre l'activité signée à chaque follower (concurrency limitée)
//!
//! ## Concurrency (Anti-Thundering Herd)
//! Utilise un `tokio::sync::Semaphore` pour limiter le nombre de
//! livraisons simultanées (défaut: 10). Empêche le pic réseau
//! si un acteur a 1000+ followers.
//!
//! ## Dette Technique
//! Fire-and-forget via `tokio::spawn`. V2 : job queue avec retry.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::federation::FederationActivity;
use domain::errors::DomainError;
use domain::ports::federation_repository::FederationRepository;
use domain::ports::federation_service::FederationService;

use super::delivery;
use super::remote_actor::RemoteActorFetcher;

/// Nombre maximum de livraisons simultanées par fanout.
/// Protège contre le thundering herd si un acteur a beaucoup de followers.
const MAX_CONCURRENT_DELIVERIES: usize = 10;

/// Service de fanout fédéré — outbox + delivery signée.
pub struct FanoutService {
    federation_repo: Arc<dyn FederationRepository>,
    remote_fetcher: Arc<RemoteActorFetcher>,
    semaphore: Arc<Semaphore>,
}

impl FanoutService {
    /// Construit le service de fanout.
    ///
    /// ## Arguments
    /// - `federation_repo` : Accès à l'outbox PostgreSQL et aux followers
    /// - `remote_fetcher` : Fetcher d'acteurs distants (pour résoudre les inboxes)
    pub fn new(
        federation_repo: Arc<dyn FederationRepository>,
        remote_fetcher: Arc<RemoteActorFetcher>,
    ) -> Self {
        Self {
            federation_repo,
            remote_fetcher,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DELIVERIES)),
        }
    }
}

#[async_trait]
impl FederationService for FanoutService {
    async fn publish_activity(
        &self,
        actor_id: &Uuid,
        activity_type: &str,
        object_type: &str,
        object_id: &str,
        activity_json: serde_json::Value,
    ) -> Result<(), DomainError> {
        let activity_id = Uuid::new_v4();

        // ── 1. Sauvegarder dans l'outbox ──
        let activity = FederationActivity {
            id: activity_id,
            actor_id: *actor_id,
            activity_type: activity_type.to_string(),
            object_type: object_type.to_string(),
            object_id: object_id.to_string(),
            activity_json: activity_json.clone(),
            published_at: Utc::now(),
        };

        self.federation_repo.save_activity(&activity).await?;

        info!(
            activity_id = %activity_id,
            activity_type = %activity_type,
            object_type = %object_type,
            "📋 Activity saved to outbox"
        );

        // ── 2. Lister les followers ──
        let followers = self.federation_repo.list_followers(actor_id).await?;

        if followers.is_empty() {
            info!(
                actor_id = %actor_id,
                activity_type = %activity_type,
                "📭 No followers — skipping fanout delivery"
            );
            return Ok(());
        }

        info!(
            actor_id = %actor_id,
            follower_count = followers.len(),
            activity_type = %activity_type,
            "📤 Starting fanout delivery to {} followers",
            followers.len()
        );

        // ── 3. Récupérer la keypair de l'acteur ──
        let keypair = self
            .federation_repo
            .get_keypair(actor_id)
            .await?
            .ok_or_else(|| {
                DomainError::BusinessRule(format!(
                    "No keypair for actor {} — cannot sign deliveries",
                    actor_id
                ))
            })?;

        // ── 4. Fanout vers chaque follower (concurrency limitée) ──
        let semaphore = self.semaphore.clone();
        let private_key = keypair.private_key_pem.clone();
        let key_id = keypair.key_id.clone();
        let remote_fetcher = self.remote_fetcher.clone();

        for follow in followers {
            let sem = semaphore.clone();
            let activity_json = activity_json.clone();
            let private_key = private_key.clone();
            let key_id = key_id.clone();
            let fetcher = remote_fetcher.clone();
            let follower_uri = follow.follower_uri.clone();

            tokio::spawn(async move {
                // Acquérir le permis du sémaphore (attend si 10 déjà en cours)
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        warn!(
                            follower = %follower_uri,
                            "⚠️ Fanout semaphore closed — skipping delivery"
                        );
                        return;
                    }
                };

                // Résoudre l'inbox du follower distant
                let target_inbox = match fetcher.fetch(&follower_uri).await {
                    Ok(profile) => {
                        if profile.inbox.is_empty() {
                            format!("{}/inbox", follower_uri)
                        } else {
                            profile.inbox
                        }
                    }
                    Err(e) => {
                        warn!(
                            follower = %follower_uri,
                            error = %e,
                            "⚠️ Failed to resolve follower inbox — using fallback"
                        );
                        format!("{}/inbox", follower_uri)
                    }
                };

                // Livrer l'activité signée
                if let Err(e) = delivery::deliver_activity(
                    activity_json,
                    &target_inbox,
                    &private_key,
                    &key_id,
                )
                .await
                {
                    warn!(
                        follower = %follower_uri,
                        target = %target_inbox,
                        error = %e,
                        "⚠️ Fanout delivery failed (non-fatal)"
                    );
                } else {
                    info!(
                        follower = %follower_uri,
                        target = %target_inbox,
                        "✅ Fanout delivery succeeded"
                    );
                }
            });
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::federation::{FederationFollow, FederationKeypair};

    // Mock FederationRepository pour les tests unitaires
    struct MockFederationRepo {
        followers: Vec<FederationFollow>,
        keypair: Option<FederationKeypair>,
        activities_saved: std::sync::Mutex<Vec<FederationActivity>>,
    }

    impl MockFederationRepo {
        fn empty() -> Self {
            Self {
                followers: vec![],
                keypair: None,
                activities_saved: std::sync::Mutex::new(vec![]),
            }
        }

        fn with_followers(followers: Vec<FederationFollow>) -> Self {
            Self {
                followers,
                keypair: Some(FederationKeypair {
                    actor_id: Uuid::new_v4(),
                    public_key_pem: "test-pub".into(),
                    private_key_pem: "test-priv".into(),
                    key_id: "test-key-id".into(),
                    created_at: Utc::now(),
                }),
                activities_saved: std::sync::Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl FederationRepository for MockFederationRepo {
        async fn get_keypair(&self, _actor_id: &Uuid) -> Result<Option<FederationKeypair>, DomainError> {
            Ok(self.keypair.clone())
        }
        async fn save_keypair(&self, _keypair: &FederationKeypair) -> Result<(), DomainError> {
            Ok(())
        }
        async fn save_follow(&self, _follow: &FederationFollow) -> Result<(), DomainError> {
            Ok(())
        }
        async fn delete_follow(&self, _follower_uri: &str, _following_actor_id: &Uuid) -> Result<bool, DomainError> {
            Ok(true)
        }
        async fn list_followers(&self, _actor_id: &Uuid) -> Result<Vec<FederationFollow>, DomainError> {
            Ok(self.followers.clone())
        }
        async fn count_followers(&self, _actor_id: &Uuid) -> Result<i64, DomainError> {
            Ok(self.followers.len() as i64)
        }
        async fn save_activity(&self, activity: &FederationActivity) -> Result<(), DomainError> {
            self.activities_saved.lock().unwrap().push(activity.clone());
            Ok(())
        }
        async fn list_activities(&self, _actor_id: &Uuid, _limit: i64) -> Result<Vec<FederationActivity>, DomainError> {
            Ok(vec![])
        }
        async fn count_activities(&self, _actor_id: &Uuid) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn count_local_users(&self) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn count_local_repos(&self) -> Result<i64, DomainError> {
            Ok(0)
        }
        // Phase 27-quater stubs
        async fn save_inbox_activity(&self, _activity: &domain::entities::federation::InboxActivity) -> Result<(), DomainError> {
            Ok(())
        }
        async fn list_inbox_activities(&self, _recipient_id: &Uuid, _limit: i64) -> Result<Vec<domain::entities::federation::InboxActivity>, DomainError> {
            Ok(vec![])
        }
        async fn count_inbox_activities(&self, _recipient_id: &Uuid) -> Result<i64, DomainError> {
            Ok(0)
        }
        async fn mark_inbox_processed(&self, _activity_id: &Uuid) -> Result<(), DomainError> {
            Ok(())
        }
        async fn list_unprocessed_inbox(&self, _limit: i64) -> Result<Vec<domain::entities::federation::InboxActivity>, DomainError> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_fanout_saves_activity_to_outbox() {
        let repo = Arc::new(MockFederationRepo::empty());
        let fetcher = Arc::new(RemoteActorFetcher::new());

        let service = FanoutService::new(repo.clone(), fetcher);
        let actor_id = Uuid::new_v4();

        let result = service
            .publish_activity(
                &actor_id,
                "Create",
                "Repository",
                "https://test.dev/repos/alice/my-lib",
                serde_json::json!({"type": "Create"}),
            )
            .await;

        assert!(result.is_ok());
        let saved = repo.activities_saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].activity_type, "Create");
        assert_eq!(saved[0].object_type, "Repository");
    }

    #[tokio::test]
    async fn test_fanout_skips_delivery_if_no_followers() {
        let repo = Arc::new(MockFederationRepo::empty());
        let fetcher = Arc::new(RemoteActorFetcher::new());

        let service = FanoutService::new(repo, fetcher);
        let actor_id = Uuid::new_v4();

        // Devrait réussir sans lancer de livraison
        let result = service
            .publish_activity(
                &actor_id,
                "Push",
                "Repository",
                "https://test.dev/repos/alice/my-lib",
                serde_json::json!({"type": "Push"}),
            )
            .await;

        assert!(result.is_ok());
    }
}
