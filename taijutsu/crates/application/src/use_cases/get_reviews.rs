//! Use Case: GetReviews — Retrouver les code reviews d'une opération.
//!
//! Use case simple de lecture — utilisé par l'API REST pour afficher
//! les reviews Oracle dans le frontend Makimono.

use std::sync::Arc;

use tracing::instrument;
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::review_repository::{OperationReview, ReviewRepository};

/// Résultat de la recherche de reviews.
pub struct ReviewsResult {
    /// Reviews trouvées (triées par date décroissante).
    pub reviews: Vec<OperationReview>,
    /// Nombre total de reviews.
    pub count: usize,
}

/// Use case: récupérer les code reviews d'une opération VCS.
pub struct GetReviewsUseCase {
    review_repo: Arc<dyn ReviewRepository>,
}

impl GetReviewsUseCase {
    pub fn new(review_repo: Arc<dyn ReviewRepository>) -> Self {
        Self { review_repo }
    }

    /// Retrouve toutes les reviews d'une opération.
    #[instrument(skip(self))]
    pub async fn execute(
        &self,
        operation_id: Uuid,
    ) -> Result<ReviewsResult, DomainError> {
        let reviews = self
            .review_repo
            .find_by_operation(&operation_id)
            .await?;

        let count = reviews.len();

        Ok(ReviewsResult { reviews, count })
    }
}
