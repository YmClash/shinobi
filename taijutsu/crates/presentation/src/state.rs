//! État partagé entre les handlers REST et gRPC.
//!
//! Contient les use cases injectés depuis le binaire principal.
//! `Clone` est cheap : tous les champs sont `Arc`.

use std::path::PathBuf;
use std::sync::Arc;

use application::use_cases::create_operation::CreateOperationUseCase;
use application::use_cases::create_repository::CreateRepositoryUseCase;
use application::use_cases::get_blob::GetBlobUseCase;
use application::use_cases::get_ipfs_content::GetIpfsContentUseCase;
use application::use_cases::get_operation::GetOperationUseCase;
use application::use_cases::get_operation_diff::GetOperationDiffUseCase;
use application::use_cases::get_reviews::GetReviewsUseCase;
use application::use_cases::get_score_history::GetScoreHistoryUseCase;
use application::use_cases::get_tree::GetTreeUseCase;
use application::use_cases::import_github_repo::ImportGitHubRepoUseCase;
use application::use_cases::list_operations::ListOperationsUseCase;
use application::use_cases::list_refs::ListRefsUseCase;
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

    // ── Phase 6 — Explorateur de Code ─────────────────────────────────
    /// Use case: lister l'arborescence d'un dépôt (Phase 6).
    pub get_tree: Arc<GetTreeUseCase>,

    /// Use case: lire le contenu d'un fichier (Phase 6).
    pub get_blob: Arc<GetBlobUseCase>,

    /// Use case: lister les branches et tags (Phase 6).
    pub list_refs: Arc<ListRefsUseCase>,

    // ── Phase 15 — Sensei (先生) ─────────────────────────────────
    /// Use case: chat conversationnel IA avec streaming.
    /// `None` si l'agent Sensei est désactivé (Ollama #2 non disponible).
    pub sensei_chat: Option<Arc<application::use_cases::sensei_chat::SenseiChatUseCase>>,

    /// URL du serveur Ollama Sensei pour les appels directs (models, warmup).
    pub sensei_ollama_url: Option<String>,

    // ── Phase 17 — Diff Colorisé ─────────────────────────────────
    /// Moteur VCS abstrait pour le diff ligne par ligne (Phase 17).
    pub vcs_engine: Arc<dyn domain::ports::vcs_engine::VcsEngine>,

    /// Repository des opérations pour le total_count (Phase 17).
    pub operation_repo: Arc<dyn domain::ports::repository::OperationRepository>,

    // ── Phase 19A — Auth & RBAC ──────────────────────────────────
    /// Service d'authentification (JWT + Argon2 + PAT).
    pub auth_service: Arc<dyn domain::ports::auth_service::AuthService>,

    /// Repository des acteurs (pour /me, PAT lookup, etc.).
    pub actor_repo: Arc<dyn domain::ports::actor_repository::ActorRepository>,

    /// Repository des dépôts (pour RBAC — is_collaborator, get_role).
    pub repo_repo: Arc<dyn domain::ports::repo_repository::RepoRepository>,

    /// Use case: inscription d'un acteur.
    pub register_actor: Arc<application::use_cases::register_actor::RegisterActorUseCase>,

    /// Use case: connexion d'un acteur.
    pub login_actor: Arc<application::use_cases::login_actor::LoginActorUseCase>,

    /// Use case: création de PAT.
    pub create_pat: Arc<application::use_cases::create_pat::CreatePatUseCase>,

    // ── Phase 19B — GitHub Import ──────────────────────────────────
    /// Use case: import d'un dépôt GitHub dans la Forge.
    pub import_github_repo: Arc<ImportGitHubRepoUseCase>,

    /// Service GitHub (API v3 + fetch injecté).
    pub github_service: Arc<dyn domain::ports::github_service::GitHubService>,

    // ── Phase 20 — GitHub OAuth ──────────────────────────────────
    /// Use case: authentification via GitHub OAuth.
    /// `None` si les variables GITHUB_CLIENT_ID/SECRET ne sont pas configurées.
    pub oauth_github: Option<Arc<application::use_cases::oauth_github::OAuthGitHubUseCase>>,

    /// URL du frontend (pour construire la redirect_uri OAuth).
    pub frontend_url: Option<String>,
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

    /// ContentStore IPFS (Genjutsu) — optionnel pour graceful degradation.
    /// Utilise par le Sync Hook pour stocker les fichiers pushes sur IPFS
    /// via `store_dag()` (Phase 12A-Fix).
    pub content_store: Option<Arc<dyn domain::ports::content_store::ContentStore>>,

    /// Racine des workspaces VCS (pour construire les chemins).
    pub workspace_root: PathBuf,

    // ── Phase 19A-Git — PAT Auth pour Git HTTP ──────────────────────
    /// Service d'authentification (SHA-256 hash pour PAT lookup).
    pub auth_service: Arc<dyn domain::ports::auth_service::AuthService>,

    /// Repository des acteurs (reverse PAT lookup → Actor).
    pub actor_repo: Arc<dyn domain::ports::actor_repository::ActorRepository>,

    /// Repository des dépôts (ownership check pour push).
    pub repo_repo: Arc<dyn domain::ports::repo_repository::RepoRepository>,
}
