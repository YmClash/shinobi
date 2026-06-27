//! Use Case: GetScoreHistory — Électrocardiogramme du Projet (Phase 9.2).
//!
//! Récupère les N derniers scores Oracle pour alimenter la sparkline
//! dans le frontend Makimono. Calcule la tendance (Rising/Falling/Stable)
//! avec une tolérance epsilon pour lisser le bruit stochastique du LLM.

use std::sync::Arc;

use serde::Serialize;
use tracing::instrument;

use domain::errors::DomainError;
use domain::ports::review_repository::{ReviewRepository, ScorePoint};

/// Tolérance epsilon pour le calcul de tendance.
///
/// Évite que la flèche ne clignote Rising/Falling au moindre
/// dixième de point de différence entre μ_recent et μ_window.
const TREND_EPSILON: f32 = 0.02;

/// Nombre de scores récents pour calculer μ_recent.
const RECENT_WINDOW: usize = 3;

/// Direction de la tendance de qualité du code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    /// μ_recent > μ_window + ε — la qualité s'améliore.
    Rising,
    /// μ_recent < μ_window - ε — la qualité se dégrade.
    Falling,
    /// |μ_recent - μ_window| ≤ ε — qualité stable.
    Stable,
}

/// Résultat de l'historique des scores.
pub struct ScoreHistoryResult {
    /// Scores triés par date décroissante (le plus récent en premier).
    pub scores: Vec<ScorePoint>,
    /// Nombre total de scores.
    pub count: usize,
    /// Moyenne globale de la fenêtre (μ_window).
    pub average: Option<f32>,
    /// Tendance calculée avec tolérance epsilon.
    pub trend: Trend,
}

/// Use case: récupérer l'historique des scores pour la sparkline.
pub struct GetScoreHistoryUseCase {
    review_repo: Arc<dyn ReviewRepository>,
}

impl GetScoreHistoryUseCase {
    pub fn new(review_repo: Arc<dyn ReviewRepository>) -> Self {
        Self { review_repo }
    }

    /// Récupère les N derniers scores et calcule la tendance.
    ///
    /// ## Algorithme de tendance (ε-tolérant)
    ///
    /// Soit N le nombre de scores dans la fenêtre :
    /// - μ_window = (1/N) × Σ S_i
    /// - μ_recent = (1/3) × Σ S_i pour les 3 derniers
    /// - Rising si μ_recent > μ_window + ε
    /// - Falling si μ_recent < μ_window - ε
    /// - Stable sinon
    #[instrument(skip(self))]
    pub async fn execute(&self, limit: usize) -> Result<ScoreHistoryResult, DomainError> {
        let scores = self.review_repo.find_recent_scores(limit).await?;
        let count = scores.len();

        if count == 0 {
            return Ok(ScoreHistoryResult {
                scores,
                count: 0,
                average: None,
                trend: Trend::Stable,
            });
        }

        // μ_window — moyenne globale de la fenêtre.
        let sum: f32 = scores.iter().map(|s| s.score).sum();
        let average = sum / count as f32;

        // μ_recent — moyenne des RECENT_WINDOW derniers scores.
        let trend = if count >= RECENT_WINDOW {
            let recent_sum: f32 = scores.iter().take(RECENT_WINDOW).map(|s| s.score).sum();
            let recent_avg = recent_sum / RECENT_WINDOW as f32;

            if recent_avg > average + TREND_EPSILON {
                Trend::Rising
            } else if recent_avg < average - TREND_EPSILON {
                Trend::Falling
            } else {
                Trend::Stable
            }
        } else {
            // Pas assez de données pour calculer une tendance.
            Trend::Stable
        };

        Ok(ScoreHistoryResult {
            scores,
            count,
            average: Some(average),
            trend,
        })
    }
}
