//! Use Case: CreateOperation — Créer une nouvelle opération VCS.
//!
//! Orchestre le flux complet :
//! 1. Demande au VcsEngine de créer le changement (→ CID)
//! 2. Construit l'entité Operation
//! 3. Persiste via le OperationRepository
//! 4. Publie un événement sur le bus (Nen/Kafka) si disponible

use std::sync::Arc;

use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::event_publisher::EventPublisher;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::VcsEngine;

/// Commande d'entrée pour le use case.
#[derive(Debug)]
pub struct CreateOperationCommand {
    pub author_id: Uuid,
    pub description: String,
    pub parent_ids: Vec<Uuid>,
    /// Fichiers à écrire dans le tree du commit.
    /// Vide = commit de métadonnées (empty tree).
    pub files: Vec<(String, Vec<u8>)>,
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
///
/// L'`EventPublisher` est optionnel : si Kafka n'est pas
/// configuré, le use case fonctionne sans publication d'événements.
pub struct CreateOperationUseCase {
    vcs_engine: Arc<dyn VcsEngine>,
    repository: Arc<dyn OperationRepository>,
    event_publisher: Option<Arc<dyn EventPublisher>>,
}

impl CreateOperationUseCase {
    /// Construit le use case avec ses dépendances injectées.
    pub fn new(
        vcs_engine: Arc<dyn VcsEngine>,
        repository: Arc<dyn OperationRepository>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
    ) -> Self {
        Self {
            vcs_engine,
            repository,
            event_publisher,
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
            .create_operation(&cmd.description, &parent_id_strings, &cmd.files)
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

        // 4. Publier l'événement sur le bus (Nen) — fire-and-forget.
        // tokio::spawn déplace la publication Kafka en background :
        // la réponse HTTP/gRPC est renvoyée IMMÉDIATEMENT après le save().
        // Le broker Kafka reçoit le message de manière asynchrone.
        // Si la publication échoue, l'opération est déjà persistée (at-most-once).
        if let Some(publisher) = self.event_publisher.clone() {
            let op_clone = operation.clone();
            tokio::spawn(async move {
                if let Err(e) = publisher.publish_operation_created(&op_clone).await {
                    warn!(
                        operation_id = %op_clone.id,
                        error = %e,
                        "Événement non publié (opération persistée malgré tout)"
                    );
                }
            });
        }

        Ok(CreateOperationResult { operation })
    }
}
