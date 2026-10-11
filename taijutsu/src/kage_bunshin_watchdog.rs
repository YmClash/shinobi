//! Watchdog Kage Bunshin — Phase 42-pre-Fix (Kage Bunshin V2) ⏰🥷
//!
//! Filet de sécurité : si une demande de heal est perdue (message Kafka
//! sauté, consumer arrêté, LLM bloqué…), le stage resterait `healing` et
//! le pipeline `running` **à vie**. Le watchdog expire ces heals.
//!
//! ## Fonctionnement
//! - Tick toutes les [`WATCHDOG_INTERVAL`] (premier tick après un intervalle
//!   complet, pour laisser le `KageBunshinConsumer` revendiquer les demandes
//!   rejouées au démarrage).
//! - Délègue à `FinalizePipelineUseCase::expire_stale_heals` : transition
//!   atomique `healing → failure`, note dans les logs du stage, puis
//!   finalisation **et annonce** (commit status + webhook `pipeline_completed`).
//! - Le consumer rafraîchit `updated_at` à chaque claim (heartbeat) : un heal
//!   en cours n'est jamais expiré tant qu'il reste dans le délai.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use application::use_cases::finalize_pipeline::FinalizePipelineUseCase;

/// Période de scrutation du watchdog.
pub const WATCHDOG_INTERVAL: Duration = Duration::from_secs(60);

/// Boucle du watchdog (à lancer via `tokio::spawn`).
pub async fn run(
    finalizer: Arc<FinalizePipelineUseCase>,
    heal_timeout: Duration,
    cancel_token: CancellationToken,
) {
    info!(
        timeout_secs = heal_timeout.as_secs(),
        interval_secs = WATCHDOG_INTERVAL.as_secs(),
        "⏰ Kage Bunshin Watchdog démarré"
    );

    let start = tokio::time::Instant::now() + WATCHDOG_INTERVAL;
    let mut ticker = tokio::time::interval_at(start, WATCHDOG_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                info!("⏰ Kage Bunshin Watchdog — shutdown gracieux");
                break;
            }
            _ = ticker.tick() => {
                match finalizer.expire_stale_heals(heal_timeout).await {
                    Ok(0) => {}
                    Ok(n) => warn!(expired = n, "⏰ Watchdog — {n} heal(s) expiré(s) et finalisé(s)"),
                    Err(e) => warn!(error = %e, "⚠️ Watchdog — scrutation échouée (nouvel essai au prochain tick)"),
                }
            }
        }
    }
}
