//! État partagé entre les handlers REST et gRPC.
//!
//! Contient les use cases injectés depuis le binaire principal.
//! `Clone` est cheap : tous les champs sont `Arc`.

use std::path::PathBuf;
use std::sync::Arc;

use application::use_cases::create_operation::CreateOperationUseCase;
use application::use_cases::create_repository::CreateRepositoryUseCase;
use application::use_cases::get_operation::GetOperationUseCase;
use application::use_cases::get_operation_diff::GetOperationDiffUseCase;
use application::use_cases::get_ipfs_content::GetIpfsContentUseCase;
use application::use_cases::get_reviews::GetReviewsUseCase;
use application::use_cases::get_score_history::GetScoreHistoryUseCase;
use application::use_cases::list_operations::ListOperationsUseCase;
use application::use_cases::list_repositories::ListRepositoriesUseCase;
use application::use_cases::resolve_repo::ResolveRepoUseCase;
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

    /// Use case: récupérer les code reviews de l'Oracle.
    pub get_reviews: Arc<GetReviewsUseCase>,

    /// Use case: historique des scores Oracle (Phase 9.2 — Sparkline).
    pub get_score_history: Arc<GetScoreHistoryUseCase>,

    /// Use case: résolution sémantique des dépôts (Phase 10C — Routes fédérées).
    pub resolve_repo: Arc<ResolveRepoUseCase>,

    /// Use case: créer un nouveau dépôt (Phase 10D — Big Bang).
    pub create_repository: Arc<CreateRepositoryUseCase>,

    /// Use case: lister les dépôts d'un acteur (Phase 10D — Préambule Makimono 5).
    pub list_repositories: Arc<ListRepositoriesUseCase>,
}

// ── Phase 12A — Git Bridge HTTP ──────────────────────────────────────

/// État dédié aux routes Git Smart HTTP Protocol (Phase 12A).
///
/// Séparé de `SharedState` car les routes Git utilisent des types
/// concrets d'infrastructure (`JujutsuEngine`, `GitCgiBackend`) plutôt
/// que des abstractions domain (traits). C'est une transgression
/// architecturale délibérée — le protocole Git est intrinsèquement
/// couplé à l'infrastructure.
#[derive(Clone)]
pub struct GitHttpState {
    /// Résolution sémantique `(owner, repo)` → `Repository`.
    pub resolve_repo: Arc<ResolveRepoUseCase>,

    /// Moteur VCS concret (type `JujutsuEngine`, pas `dyn VcsEngine`).
    /// Nécessaire pour `reload_repo()` et `git_repo_path()`.
    pub vcs_engine: Arc<infrastructure::vcs::jujutsu_engine::JujutsuEngine>,

    /// Backend CGI Git (`git http-backend`).
    pub git_cgi: Arc<infrastructure::vcs::git_cgi::GitCgiBackend>,

    /// Publication Kafka (étincelle post-push) — optionnel.
    pub event_publisher: Option<Arc<dyn domain::ports::event_publisher::EventPublisher>>,

    /// Persistence des opérations (combler le vide PostgreSQL).
    pub operation_repo: Arc<dyn domain::ports::repository::OperationRepository>,

    /// Racine des workspaces VCS (pour construire les chemins).
    pub workspace_root: PathBuf,
}
