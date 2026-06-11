//! Services gRPC Ninpo — Implémentation du service ShinobiService.
//!
//! Adaptateur primaire gRPC utilisant Tonic.
//! Les requêtes gRPC sont traduites en appels aux use cases
//! de la couche application via `SharedState`.

use tonic::{Request, Response, Status};
use tracing::info;
use uuid::Uuid;

use application::use_cases::create_operation::CreateOperationCommand;
use application::use_cases::list_operations::ListFilter;

use crate::errors::domain_error_to_status;
use crate::state::SharedState;

// Code généré par tonic-build à partir de proto/shinobi.proto.
pub mod proto {
    tonic::include_proto!("shinobi.ninpo");
}

use proto::shinobi_service_server::ShinobiService;
use proto::{
    CreateOperationRequest, GetOperationRequest, ListOperationsRequest,
    ListOperationsResponse, OperationResponse, PingRequest, PingResponse,
};

/// Implémentation du service gRPC ShinobiService.
///
/// Reçoit `SharedState` contenant les use cases injectés.
pub struct ShinobiServiceImpl {
    state: SharedState,
}

impl ShinobiServiceImpl {
    /// Construit le service avec les use cases injectés.
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

/// Convertit une `Operation` domaine en réponse Protobuf.
fn operation_to_proto(op: domain::entities::operation::Operation) -> OperationResponse {
    OperationResponse {
        id: op.id.to_string(),
        author_id: op.author_id.to_string(),
        content_id: op.content_id.into_inner(),
        description: op.description,
        parent_ids: op.parent_ids.iter().map(|id| id.to_string()).collect(),
        created_at: op.created_at.timestamp(),
    }
}

#[tonic::async_trait]
impl ShinobiService for ShinobiServiceImpl {
    /// Ping — Vérification de la disponibilité du serveur gRPC.
    async fn ping(
        &self,
        _request: Request<PingRequest>,
    ) -> Result<Response<PingResponse>, Status> {
        info!("Ninpo: Ping reçu");

        let response = PingResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            status: "operational".to_string(),
            uptime_seconds: 0, // TODO: calcul réel via Instant::now()
        };

        Ok(Response::new(response))
    }

    /// CreateOperation — Crée une opération VCS via le protocole Ninpo.
    async fn create_operation(
        &self,
        request: Request<CreateOperationRequest>,
    ) -> Result<Response<OperationResponse>, Status> {
        let req = request.into_inner();

        info!(
            author_id = %req.author_id,
            description = %req.description,
            "Ninpo: CreateOperation reçu"
        );

        let author_id = req
            .author_id
            .parse::<Uuid>()
            .map_err(|e| Status::invalid_argument(format!("author_id invalide: {e}")))?;

        let parent_ids: Vec<Uuid> = req
            .parent_ids
            .iter()
            .map(|id| {
                id.parse::<Uuid>()
                    .map_err(|e| Status::invalid_argument(format!("parent_id invalide: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let cmd = CreateOperationCommand {
            author_id,
            description: req.description,
            parent_ids,
            files: vec![],
        };

        let result = self
            .state
            .create_operation
            .execute(cmd)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(operation_to_proto(result.operation)))
    }

    /// GetOperation — Retrouver une opération par son ID.
    async fn get_operation(
        &self,
        request: Request<GetOperationRequest>,
    ) -> Result<Response<OperationResponse>, Status> {
        let req = request.into_inner();
        info!(id = %req.id, "Ninpo: GetOperation reçu");

        let id = req
            .id
            .parse::<Uuid>()
            .map_err(|e| Status::invalid_argument(format!("id invalide: {e}")))?;

        let operation = self
            .state
            .get_operation
            .execute(id)
            .await
            .map_err(domain_error_to_status)?;

        Ok(Response::new(operation_to_proto(operation)))
    }

    /// ListOperations — Lister les opérations avec filtrage optionnel.
    async fn list_operations(
        &self,
        request: Request<ListOperationsRequest>,
    ) -> Result<Response<ListOperationsResponse>, Status> {
        let req = request.into_inner();
        info!(limit = req.limit, author_id = %req.author_id, "Ninpo: ListOperations reçu");

        let filter = if req.author_id.is_empty() {
            let limit = if req.limit > 0 { req.limit as usize } else { 50 };
            ListFilter::Recent { limit }
        } else {
            let author_id = req
                .author_id
                .parse::<Uuid>()
                .map_err(|e| Status::invalid_argument(format!("author_id invalide: {e}")))?;
            ListFilter::ByAuthor { author_id }
        };

        let operations = self
            .state
            .list_operations
            .execute(filter)
            .await
            .map_err(domain_error_to_status)?;

        let response = ListOperationsResponse {
            operations: operations.into_iter().map(operation_to_proto).collect(),
        };

        Ok(Response::new(response))
    }
}
