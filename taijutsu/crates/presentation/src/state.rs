//! État partagé entre les handlers REST et gRPC.
//!
//! Contient les use cases injectés depuis le binaire principal.
//! `Clone` est cheap : tous les champs sont `Arc`.

use std::sync::Arc;

use application::use_cases::create_operation::CreateOperationUseCase;
use application::use_cases::get_operation::GetOperationUseCase;
use application::use_cases::get_operation_diff::GetOperationDiffUseCase;
use application::use_cases::get_ipfs_content::GetIpfsContentUseCase;
use application::use_cases::list_operations::ListOperationsUseCase;
use application::use_cases::search_chunks::SearchChunksUseCase;

/// État applicatif partagé entre les couches de présentation.
///
/// Construit dans `main.rs` puis injecté dans Axum (`.with_state()`)
/// et dans Tonic (constructeur `ShinobiServiceImpl::new()`).
#[derive(Clone)]
pub struct SharedState {
    /// Use case: créer une opération VCS.
    pub create_operation: Arc<CreateOperationUseCase>,

    /// Use case: retrouver une opération par ID.
    pub get_operation: Arc<GetOperationUseCase>,

    /// Use case: lister les opérations avec filtrage.
    pub list_operations: Arc<ListOperationsUseCase>,

    /// Use case: interroger la mémoire sémantique de l'IA.
    pub search_chunks: Arc<SearchChunksUseCase>,

    /// Use case: récupérer le diff d'une opération VCS.
    pub get_operation_diff: Arc<GetOperationDiffUseCase>,

    /// Use case: récupérer le contenu IPFS d'une opération.
    pub get_ipfs_content: Arc<GetIpfsContentUseCase>,
}

