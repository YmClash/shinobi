//! Services gRPC Ninpo — Implémentation du service ShinobiService.
//!
//! Adaptateur primaire gRPC utilisant Tonic.
//! Les requêtes gRPC sont traduites en appels aux use cases
//! de la couche application.

use tonic::{Request, Response, Status};
use tracing::info;

// Code généré par tonic-build à partir de proto/shinobi.proto.
pub mod proto {
    tonic::include_proto!("shinobi.ninpo");
}

use proto::shinobi_service_server::ShinobiService;
use proto::{
    CreateOperationRequest, CreateOperationResponse, PingRequest, PingResponse,
};

/// Implémentation du service gRPC ShinobiService.
///
/// En phase 1, les use cases seront injectés directement.
/// En production, un AppState partagé fournira les dépendances.
#[derive(Debug, Default)]
pub struct ShinobiServiceImpl;

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
    ) -> Result<Response<CreateOperationResponse>, Status> {
        let req = request.into_inner();

        info!(
            author_id = %req.author_id,
            description = %req.description,
            "Ninpo: CreateOperation reçu"
        );

        // TODO (Phase 2): Injecter le CreateOperationUseCase
        // et déléguer la logique métier à la couche application.
        let response = CreateOperationResponse {
            operation_id: uuid::Uuid::new_v4().to_string(),
            content_id: format!("bafk_{}", uuid::Uuid::new_v4().simple()),
            created_at: chrono::Utc::now().timestamp(),
        };

        Ok(Response::new(response))
    }
}
