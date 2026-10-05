//! Port: IssueRepository — Contrat de persistence des Issues/Tickets.
//!
//! Ce trait abstrait le stockage des issues, commentaires, événements et labels.
//! L'adaptateur concret dans infrastructure/ implémente la persistence PostgreSQL.
//!
//! ## Compteur Partagé
//! `next_number()` utilise le compteur unifié `repo_counters.next_ticket_number`
//! partagé avec les Merge Requests — garantit l'unicité de `#ID` par dépôt.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::issue::{
    Issue, IssueComment, IssueEvent, IssueLabel, IssueStatus,
};
use crate::errors::DomainError;

/// Contrat de persistence pour les Issues/Tickets.
#[async_trait]
pub trait IssueRepository: Send + Sync {
    // ── Issue CRUD ──────────────────────────────────────────────────────

    /// Persiste une nouvelle issue.
    async fn save(&self, issue: &Issue) -> Result<(), DomainError>;

    /// Retrouve une issue par son ID.
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<Issue>, DomainError>;

    /// Retrouve une issue par son numéro dans un repo.
    async fn find_by_repo_and_number(
        &self,
        repo_id: &Uuid,
        number: i32,
    ) -> Result<Option<Issue>, DomainError>;

    /// Liste les issues d'un repo avec filtre optionnel par status.
    async fn list_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<IssueStatus>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Issue>, DomainError>;

    /// Met à jour le status d'une issue (transition d'état).
    async fn update_status(
        &self,
        id: &Uuid,
        status: IssueStatus,
        closed_by: Option<Uuid>,
        closed_at: Option<DateTime<Utc>>,
    ) -> Result<bool, DomainError>;

    /// Met à jour le titre d'une issue.
    async fn update_title(&self, id: &Uuid, title: &str) -> Result<bool, DomainError>;

    /// Met à jour le corps (description) d'une issue.
    async fn update_body(&self, id: &Uuid, body: Option<&str>) -> Result<bool, DomainError>;

    /// Met à jour l'assigné d'une issue.
    async fn update_assignee(&self, id: &Uuid, assignee_id: Option<Uuid>) -> Result<bool, DomainError>;

    /// Obtient le prochain numéro de ticket pour un repo (atomique, anti race-condition).
    ///
    /// Utilise un UPSERT atomique sur `repo_counters.next_ticket_number` :
    /// ```sql
    /// INSERT INTO repo_counters (repository_id, next_ticket_number)
    /// VALUES ($1, 2)
    /// ON CONFLICT (repository_id)
    /// DO UPDATE SET next_ticket_number = repo_counters.next_ticket_number + 1
    /// RETURNING next_ticket_number - 1
    /// ```
    /// Ce compteur est partagé avec les Merge Requests.
    async fn next_number(&self, repo_id: &Uuid) -> Result<i32, DomainError>;

    /// Compte les issues d'un repo avec filtre optionnel par status.
    async fn count_by_repo(
        &self,
        repo_id: &Uuid,
        status: Option<IssueStatus>,
    ) -> Result<i64, DomainError>;

    // ── Comments ────────────────────────────────────────────────────────

    /// Persiste un commentaire.
    async fn save_comment(&self, comment: &IssueComment) -> Result<(), DomainError>;

    /// Liste les commentaires d'une issue (ordre chronologique).
    async fn list_comments(&self, issue_id: &Uuid) -> Result<Vec<IssueComment>, DomainError>;

    /// Met à jour le corps d'un commentaire.
    async fn update_comment(&self, comment_id: &Uuid, body: &str) -> Result<bool, DomainError>;

    /// Supprime un commentaire.
    async fn delete_comment(&self, comment_id: &Uuid) -> Result<bool, DomainError>;

    // ── Events (Timeline) ───────────────────────────────────────────────

    /// Persiste un événement de timeline.
    async fn save_event(&self, event: &IssueEvent) -> Result<(), DomainError>;

    /// Liste les événements d'une issue (ordre chronologique).
    async fn list_events(&self, issue_id: &Uuid) -> Result<Vec<IssueEvent>, DomainError>;

    // ── Labels ──────────────────────────────────────────────────────────

    /// Persiste un label.
    async fn save_label(&self, label: &IssueLabel) -> Result<(), DomainError>;

    /// Liste les labels d'un repo.
    async fn list_labels(&self, repo_id: &Uuid) -> Result<Vec<IssueLabel>, DomainError>;

    /// Supprime un label.
    async fn delete_label(&self, label_id: &Uuid) -> Result<bool, DomainError>;

    /// Assigne un label à une issue.
    async fn add_label_to_issue(&self, issue_id: &Uuid, label_id: &Uuid) -> Result<(), DomainError>;

    /// Retire un label d'une issue.
    async fn remove_label_from_issue(&self, issue_id: &Uuid, label_id: &Uuid) -> Result<(), DomainError>;

    /// Récupère les labels d'une issue.
    async fn get_issue_labels(&self, issue_id: &Uuid) -> Result<Vec<IssueLabel>, DomainError>;
}
