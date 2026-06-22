//! Tests unitaires pour GetScoreHistoryUseCase (Phase 9.2).
//!
//! Verrouille l'algorithme de tendance ε-tolérant :
//! - Vide : 0 scores → Stable, average=None
//! - Score unique : < RECENT_WINDOW → Stable (pas assez de données)
//! - Deux scores : encore < RECENT_WINDOW → Stable
//! - Trend Rising : les 3 derniers significativement > moyenne globale
//! - Trend Falling : les 3 derniers significativement < moyenne globale
//! - Trend Stable : différence dans la zone morte ε=0.02
//! - Epsilon boundary : exactement à la frontière → Stable (≤ ε)

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::review_repository::{
    OperationReview, ReviewRepository, ScorePoint,
};

use crate::use_cases::get_score_history::{GetScoreHistoryUseCase, Trend};

// ── Mock ReviewRepository ──────────────────────────────────

/// Mock qui retourne des ScorePoints prédéfinis.
struct MockScoreRepo {
    scores: Vec<ScorePoint>,
}

impl MockScoreRepo {
    fn new(scores: Vec<ScorePoint>) -> Self {
        Self { scores }
    }

    /// Helper : crée N ScorePoints à partir d'un slice de f32.
    /// Les scores sont donnés dans l'ordre DESC (le premier = le plus récent).
    fn from_scores(values: &[f32]) -> Self {
        let scores = values
            .iter()
            .enumerate()
            .map(|(i, &score)| ScorePoint {
                operation_id: Uuid::new_v4(),
                score,
                // Dates décroissantes : le premier est le plus récent.
                created_at: Utc.with_ymd_and_hms(2026, 6, 21, 12, 0, 0).unwrap()
                    - chrono::Duration::hours(i as i64),
            })
            .collect();
        Self { scores }
    }
}

#[async_trait]
impl ReviewRepository for MockScoreRepo {
    async fn save_review(&self, _review: &OperationReview) -> Result<(), DomainError> {
        Ok(())
    }
    async fn find_by_operation(
        &self,
        _operation_id: &Uuid,
    ) -> Result<Vec<OperationReview>, DomainError> {
        Ok(vec![])
    }
    async fn delete_by_operation(&self, _operation_id: &Uuid) -> Result<u64, DomainError> {
        Ok(0)
    }
    async fn find_recent_scores(&self, limit: usize) -> Result<Vec<ScorePoint>, DomainError> {
        Ok(self.scores.iter().take(limit).cloned().collect())
    }
}

// ── Tests ──────────────────────────────────────────────────

/// Aucun score en base → count=0, average=None, trend=Stable.
#[tokio::test]
async fn test_empty_scores_returns_stable() {
    let repo = Arc::new(MockScoreRepo::new(vec![]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 0);
    assert!(result.average.is_none());
    assert_eq!(result.trend, Trend::Stable);
    assert!(result.scores.is_empty());
}

/// Un seul score → count=1, average=Some(score), trend=Stable
/// (pas assez de données pour calculer une tendance, < RECENT_WINDOW=3).
#[tokio::test]
async fn test_single_score_returns_stable() {
    let repo = Arc::new(MockScoreRepo::from_scores(&[0.85]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 1);
    assert!((result.average.unwrap() - 0.85).abs() < 0.001);
    assert_eq!(result.trend, Trend::Stable);
}

/// Deux scores → count=2, trend=Stable (< RECENT_WINDOW=3).
#[tokio::test]
async fn test_two_scores_returns_stable() {
    let repo = Arc::new(MockScoreRepo::from_scores(&[0.90, 0.70]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 2);
    assert!((result.average.unwrap() - 0.80).abs() < 0.001);
    assert_eq!(result.trend, Trend::Stable);
}

/// Trend Rising : les 3 derniers commits sont nettement meilleurs que l'historique.
///
/// Scores (DESC) : [0.95, 0.92, 0.90, 0.60, 0.55, 0.50]
/// μ_window = (0.95+0.92+0.90+0.60+0.55+0.50) / 6 = 0.7367
/// μ_recent = (0.95+0.92+0.90) / 3 = 0.9233
/// μ_recent (0.9233) > μ_window (0.7367) + ε (0.02) → Rising ✅
#[tokio::test]
async fn test_trend_rising() {
    let repo = Arc::new(MockScoreRepo::from_scores(&[
        0.95, 0.92, 0.90, // 3 derniers — haute qualité
        0.60, 0.55, 0.50, // historique — basse qualité
    ]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 6);
    assert_eq!(result.trend, Trend::Rising);
    // Vérifier μ_window
    let expected_avg = (0.95 + 0.92 + 0.90 + 0.60 + 0.55 + 0.50) / 6.0;
    assert!((result.average.unwrap() - expected_avg).abs() < 0.001);
}

/// Trend Falling : les 3 derniers commits ont dégradé la qualité.
///
/// Scores (DESC) : [0.40, 0.45, 0.42, 0.90, 0.88, 0.85]
/// μ_window = (0.40+0.45+0.42+0.90+0.88+0.85) / 6 = 0.65
/// μ_recent = (0.40+0.45+0.42) / 3 = 0.4233
/// μ_recent (0.4233) < μ_window (0.65) - ε (0.02) → Falling ✅
#[tokio::test]
async fn test_trend_falling() {
    let repo = Arc::new(MockScoreRepo::from_scores(&[
        0.40, 0.45, 0.42, // 3 derniers — qualité dégradée
        0.90, 0.88, 0.85, // historique — haute qualité
    ]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 6);
    assert_eq!(result.trend, Trend::Falling);
}

/// Trend Stable : les 3 derniers sont proches de la moyenne (dans la zone morte ε).
///
/// Scores (DESC) : [0.81, 0.80, 0.79, 0.80, 0.81, 0.80]
/// μ_window = (0.81+0.80+0.79+0.80+0.81+0.80) / 6 = 0.8017
/// μ_recent = (0.81+0.80+0.79) / 3 = 0.80
/// |μ_recent - μ_window| = 0.0017 ≤ ε (0.02) → Stable ✅
#[tokio::test]
async fn test_trend_stable_within_epsilon() {
    let repo = Arc::new(MockScoreRepo::from_scores(&[
        0.81, 0.80, 0.79, // 3 derniers — très proches de la moyenne
        0.80, 0.81, 0.80, // historique — même zone
    ]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 6);
    assert_eq!(result.trend, Trend::Stable);
}

/// Boundary test : exactement à ε = 0.02 au-dessus → Stable (pas Rising).
///
/// On construit des données où μ_recent - μ_window = exactement ε.
/// La condition Rising est STRICTEMENT > (pas >=), donc → Stable.
///
/// Scores : 3 scores identiques → μ_recent = μ_window → diff = 0 → Stable.
#[tokio::test]
async fn test_trend_epsilon_boundary_exact() {
    let repo = Arc::new(MockScoreRepo::from_scores(&[
        0.80, 0.80, 0.80, // tous identiques
    ]));
    let use_case = GetScoreHistoryUseCase::new(repo);

    let result = use_case.execute(10).await.unwrap();

    assert_eq!(result.count, 3);
    // μ_window = 0.80, μ_recent = 0.80, diff = 0 → Stable
    assert_eq!(result.trend, Trend::Stable);
    assert!((result.average.unwrap() - 0.80).abs() < 0.001);
}
