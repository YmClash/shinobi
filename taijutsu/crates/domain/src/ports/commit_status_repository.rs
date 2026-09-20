//! Port: CommitStatusRepository — Phase 39 (Le Pont CI/CD) 🌉
//!
//! Contrat d'accès aux statuts de commit CI/CD.
//! L'adaptateur concret dans infrastructure/ implémente le stockage PostgreSQL.
//!
//! ## Opérations principales
//! - **Upsert** : Crée ou met à jour un statut (clé: repo_id + commit_id + context)
//! - **List** : Liste tous les statuts d'un commit (pour le combined status)
//! - **Find** : Retrouve un statut spécifique par contexte

use async_trait::async_trait;
use uuid::Uuid;

use crate::entities::commit_status::CommitStatus;
use crate::errors::DomainError;

/// Contrat d'accès aux statuts de commit CI/CD.
#[async_trait]
pub trait CommitStatusRepository: Send + Sync {
    /// Crée ou met à jour un statut de commit.
    ///
    /// Utilise un UPSERT sur la clé unique `(repository_id, commit_id, context)`.
    /// Si un statut existe déjà pour ce triplet, il est mis à jour.
    /// Sinon, un nouveau statut est créé.
    ///
    /// Retourne le statut créé/mis à jour avec son ID final.
    async fn upsert(&self, status: &CommitStatus) -> Result<CommitStatus, DomainError>;

    /// Liste tous les statuts d'un commit (tous contextes confondus).
    ///
    /// Ordonnés par `updated_at DESC` (le plus récent en premier).
    /// Utilisé par le badge UI pour calculer le combined status.
    async fn list_by_commit(
        &self,
        repository_id: &Uuid,
        commit_id: &str,
    ) -> Result<Vec<CommitStatus>, DomainError>;

    /// Retrouve un statut spécifique par son contexte.
    ///
    /// Utilisé pour vérifier si un UPSERT est une création ou une mise à jour.
    async fn find_by_context(
        &self,
        repository_id: &Uuid,
        commit_id: &str,
        context: &str,
    ) -> Result<Option<CommitStatus>, DomainError>;
}
