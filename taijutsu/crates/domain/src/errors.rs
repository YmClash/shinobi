//! Erreurs du domaine — Types d'erreurs métier typées.
//!
//! Toutes les erreurs du cœur sont définies ici.
//! Les adaptateurs peuvent convertir leurs erreurs techniques
//! en `DomainError` via les implémentations `From`.

use thiserror::Error;
use uuid::Uuid;

/// Erreurs métier du domaine SHINOBI.
#[derive(Debug, Error)]
pub enum DomainError {
    /// Entité introuvable par son identifiant.
    #[error("Entité introuvable: {entity_type} [{id}]")]
    NotFound {
        entity_type: &'static str,
        id: Uuid,
    },

    /// Violation d'une règle métier.
    #[error("Règle métier violée: {0}")]
    BusinessRule(String),

    /// Conflit de concurrence (ex: modification simultanée).
    #[error("Conflit de concurrence: {0}")]
    Conflict(String),

    /// Erreur de persistence (propagée depuis l'adaptateur Fūinjutsu).
    #[error("Erreur de persistence: {0}")]
    Persistence(String),

    /// Erreur du moteur VCS (propagée depuis l'adaptateur jj-lib).
    #[error("Erreur VCS: {0}")]
    VcsError(String),

    /// Erreur du stockage distribué (propagée depuis l'adaptateur Genjutsu).
    #[error("Erreur de stockage distribué: {0}")]
    StorageError(String),

    /// Commit introuvable dans le repo VCS.
    #[error("Commit introuvable: {id}")]
    CommitNotFound { id: String },

    /// Accès refusé (multi-tenant — Phase 10A).
    #[error("Accès refusé: {0}")]
    Forbidden(String),

    /// Entrée en doublon (ex: handle déjà pris, nom de repo existant).
    #[error("Doublon détecté: {0}")]
    Duplicate(String),

    /// Erreur interne inattendue.
    #[error("Erreur interne: {0}")]
    Internal(String),
}
