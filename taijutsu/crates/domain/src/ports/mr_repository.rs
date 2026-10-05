//! Port: MrRepository — Contrat de persistence des Merge Requests.
//!
//! Ce trait abstrait le stockage des MR, reviews et événements.
//! L'adaptateur concret dans infrastructure/ implémente la persistence PostgreSQL.
//!
//! ## Anti Race-Condition
//! `next_number()` utilise un compteur atomique (table `repo_counters`)
//! au lieu de `MAX(number) + 1` pour garantir l'unicité sous concurrence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::merge_request::{
    MergeRequest, MrEvent, MrReview, MrStatus,
};
use crate::errors::DomainError;

/// Contrat de persistence pour les Merge Requests.
#[async_trait]
pub trait MrRepository: Send + Sync {
    // ── MergeRequest CRUD ───────────────────────────────────────────────

    /// Persiste une nouvelle MR.
    async fn save(&self, mr: &MergeRequest) -> Result<(), DomainError>;

    /// Retrouve une MR par son ID.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<MergeRequest>, DomainError>;

    /// Retrouve une MR par son numéro dans un repo.
    async fn find_by_repo_and_number(
        &self,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<Option<MergeRequest>, DomainError>;

    /// Liste les MR d'un repo avec filtre optionnel par status.
    async fn list_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<MrStatus>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MergeRequest>, DomainError>;

    /// Met à jour le status d'une MR (transition d'état).
    async fn update_status(
        &self,
        id: &Uuid,
        status: MrStatus,
        merged_by: Option<Uuid>,
        merged_at: Option<DateTime<Utc>>,
        closed_at: Option<DateTime<Utc>>,
    ) -> Result<bool, DomainError>;

    /// Obtient le prochain numéro de MR pour un repo (atomique, anti race-condition).
    ///
    /// Utilise un UPSERT atomique sur `repo_counters` :
    /// ```sql
    /// INSERT INTO repo_counters (repository_id, next_mr_number)
    /// VALUES ($1, 2)
    /// ON CONFLICT (repository_id)
    /// DO UPDATE SET next_mr_number = repo_counters.next_mr_number + 1
    /// RETURNING next_mr_number - 1
    /// ```
    async fn next_number(&self, repo_id: &Uuid) -> Result<i32, DomainError>;

    /// Vérifie qu'aucune MR ouverte n'existe pour la même paire de branches.
    async fn find_open_by_branches(
        &self,
        repo_id: &Uuid,
        source: &str,
        target: &str,
    ) -> Result<Option<MergeRequest>, DomainError>;

    /// Compte les MR d'un repo avec filtre optionnel par status.
    async fn count_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<MrStatus>,
    ) -> Result<i64, DomainError>;

    // ── Reviews ─────────────────────────────────────────────────────────

    /// Persiste une review.
    async fn save_review(&self, review: &MrReview) -> Result<(), DomainError>;

    /// Liste les reviews d'une MR.
    async fn list_reviews(&self, mr_id: &Uuid) -> Result<Vec<MrReview>, DomainError>;

    // ── Events (Timeline) ───────────────────────────────────────────────

    /// Persiste un événement de timeline.
    async fn save_event(&self, event: &MrEvent) -> Result<(), DomainError>;

    /// Liste les événements d'une MR (ordre chronologique).
    async fn list_events(&self, mr_id: &Uuid) -> Result<Vec<MrEvent>, DomainError>;
}
