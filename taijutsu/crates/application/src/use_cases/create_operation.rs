//! Use Case: CreateOperation — Créer une nouvelle opération VCS.
//!
//! Orchestre le flux complet :
//! 1. Demande au VcsEngine de créer le changement (→ CID)
//! 2. Construit l'entité Operation
//! 3. Persiste via le OperationRepository

use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::VcsEngine;

/// Commande d'entrée pour le use case.
#[derive(Debug)]
pub struct CreateOperationCommand {
    pub author_id: Uuid,
    pub description: String,
    pub parent_ids: Vec<Uuid>,
}

/// Résultat du use case.
#[derive(Debug)]
pub struct CreateOperationResult {
    pub operation: Operation,
}

/// Use case: créer une opération VCS et la persister.
///
/// Reçoit les ports en injection (Arc<dyn Trait>),
/// garantissant l'inversion de dépendance.
pub struct CreateOperationUseCase {
    vcs_engine: Arc<dyn VcsEngine>,
    repository: Arc<dyn OperationRepository>,
}

impl CreateOperationUseCase {
    /// Construit le use case avec ses dépendances injectées.
    pub fn new(
        vcs_engine: Arc<dyn VcsEngine>,
        repository: Arc<dyn OperationRepository>,
    ) -> Self {
        Self {
            vcs_engine,
            repository,
        }
    }

    /// Exécute le cas d'usage.
    pub async fn execute(
        &self,
        cmd: CreateOperationCommand,
    ) -> Result<CreateOperationResult, DomainError> {
        // 1. Créer le changement dans le moteur VCS → obtenir le CID.
        let parent_id_strings: Vec<String> =
            cmd.parent_ids.iter().map(|id| id.to_string()).collect();

        let content_id = self
            .vcs_engine
            .create_operation(&cmd.description, &parent_id_strings)
            .await?;

        info!(
            author_id = %cmd.author_id,
            content_id = %content_id,
            "Opération VCS créée dans le moteur"
        );

        // 2. Construire l'entité domaine.
        let operation = Operation::new(
            cmd.author_id,
            content_id,
            cmd.description,
            cmd.parent_ids,
        );

        // 3. Persister l'opération.
        self.repository.save(&operation).await?;

        info!(
            operation_id = %operation.id,
            "Opération persistée avec succès"
        );

        Ok(CreateOperationResult { operation })
    }
}
