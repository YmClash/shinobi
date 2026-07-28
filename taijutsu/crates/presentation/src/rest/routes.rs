//! Routes REST — Routeurs Axum pour l'API HTTP.
//!
//! Point d'entrée HTTP du système SHINOBI.
//! Les use cases sont injectés via `SharedState` (Axum State extractor).

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router, routing::get, routing::post};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use application::use_cases::create_operation::CreateOperationCommand;
use application::use_cases::create_repository::CreateRepositoryCommand;
use application::use_cases::import_github_repo::ImportGitHubRepoCommand;
use application::use_cases::list_operations::ListFilter;
use application::use_cases::search_chunks::ChunkSearchFilter;
use application::use_cases::sensei_chat::{ChatMessage, SenseiChatRequest};
use domain::entities::operation::Operation;
use domain::entities::repository::Visibility;
use domain::errors::DomainError;
use domain::ports::chunk_repository::{SimilarChunk, StoredChunk};
use domain::ports::review_repository::OperationReview;
use domain::ports::vcs_engine::{EntryKind, RefKind};

use crate::errors::AppError;
use crate::rest::auth_middleware::{AuthUser, MaybeAuth};
use crate::state::SharedState;

// ─── Types Request / Response ────────────────────

/// Réponse du health check.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
}

/// Corps de la requête POST /api/v1/operations.
#[derive(Debug, Deserialize)]
pub struct CreateOperationBody {
    pub author_id: Uuid,
    /// Identifiant du dépôt cible (Phase 10B — multi-tenant).
    /// Défaut: DEFAULT_REPO_ID pour la rétro-compatibilité MVP.
    #[serde(default)]
    pub repository_id: Option<Uuid>,
    pub description: String,
    #[serde(default)]
    pub parent_ids: Vec<Uuid>,
    /// Fichiers à inclure dans l'opération (optionnel).
    #[serde(default)]
    pub files: Vec<FileEntryBody>,
}

/// Entrée de fichier dans le body de la requête.
#[derive(Debug, Deserialize)]
pub struct FileEntryBody {
    /// Chemin du fichier (ex: "src/main.rs").
    pub path: String,
    /// Contenu encodé en base64 (RFC 4648).
    pub content_b64: String,
}

/// Paramètres de query pour GET /api/v1/operations.
#[derive(Debug, Deserialize)]
pub struct ListOperationsQuery {
    pub limit: Option<usize>,
    pub author_id: Option<Uuid>,
    /// Identifiant du dépôt (Phase 10B — multi-tenant).
    /// Défaut: DEFAULT_REPO_ID pour la rétro-compatibilité MVP.
    pub repository_id: Option<Uuid>,
}

/// Paramètres de query pour GET /api/v1/operations/:id/chunks.
#[derive(Debug, Deserialize)]
pub struct ChunksQuery {
    /// Filtre optionnel par chemin de fichier (ex: "src/main.rs").
    pub file: Option<String>,
}

/// Paramètres de query pour GET /api/v1/chunks/search.
#[derive(Debug, Deserialize)]
pub struct SearchChunksQuery {
    /// Nom du symbole recherché.
    pub name: String,
}

/// Body de la requête POST /api/v1/chunks/semantic-search.
#[derive(Debug, Deserialize)]
pub struct SemanticSearchBody {
    /// Requête en langage naturel.
    pub query: String,
    /// Nombre maximum de résultats (défaut: 10).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Score minimum de similarité (0.0 à 1.0, défaut: 0.5).
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

fn default_limit() -> usize {
    10
}
fn default_threshold() -> f32 {
    0.5
}

/// Réponse JSON pour une opération.
#[derive(Debug, Serialize)]
pub struct OperationJson {
    pub id: Uuid,
    pub author_id: Uuid,
    /// Identifiant du dépôt multi-tenant (Phase 10B).
    pub repository_id: Uuid,
    pub content_id: String,
    /// CID IPFS distribué — null si non synchronisé (Genjutsu).
    pub ipfs_content_id: Option<String>,
    pub description: String,
    pub parent_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl From<Operation> for OperationJson {
    fn from(op: Operation) -> Self {
        Self {
            id: op.id,
            author_id: op.author_id,
            repository_id: op.repository_id,
            content_id: op.content_id.into_inner(),
            ipfs_content_id: op.ipfs_content_id.map(|cid| cid.into_inner()),
            description: op.description,
            parent_ids: op.parent_ids,
            created_at: op.created_at,
        }
    }
}

/// Réponse JSON pour un fragment sémantique.
#[derive(Debug, Serialize)]
pub struct ChunkJson {
    pub kind: String,
    pub name: Option<String>,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub file_path: String,
    pub language: String,
}

impl From<StoredChunk> for ChunkJson {
    fn from(chunk: StoredChunk) -> Self {
        Self {
            kind: chunk.kind,
            name: chunk.name,
            content: chunk.content,
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            file_path: chunk.file_path,
            language: chunk.language,
        }
    }
}

/// Réponse JSON pour un fragment sémantique avec score de similarité (Phase 7A).
#[derive(Debug, Serialize)]
pub struct SemanticChunkJson {
    pub kind: String,
    pub name: Option<String>,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub file_path: String,
    pub language: String,
    pub similarity: f32,
}

impl From<SimilarChunk> for SemanticChunkJson {
    fn from(similar: SimilarChunk) -> Self {
        Self {
            kind: similar.chunk.kind,
            name: similar.chunk.name,
            content: similar.chunk.content,
            start_line: similar.chunk.start_line,
            end_line: similar.chunk.end_line,
            file_path: similar.chunk.file_path,
            language: similar.chunk.language,
            similarity: similar.similarity,
        }
    }
}

/// Réponse JSON pour une code review IA (Phase 9 — Oracle).
#[derive(Debug, Serialize)]
pub struct ReviewJson {
    pub id: Uuid,
    pub reviewer: String,
    pub model: String,
    pub summary: String,
    pub content: String,
    pub score: Option<f32>,
    pub duration_ms: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<OperationReview> for ReviewJson {
    fn from(review: OperationReview) -> Self {
        Self {
            id: review.id,
            reviewer: review.reviewer,
            model: review.model,
            summary: review.summary,
            content: review.content,
            score: review.score,
            duration_ms: review.duration_ms,
            created_at: review.created_at,
        }
    }
}

// ─── Routeur ─────────────────────────────────────

/// Construit le routeur Axum principal avec les use cases injectés.
///
/// Intègre automatiquement le middleware Prometheus pour les métriques HTTP.
/// La route `/metrics` expose les métriques au format Prometheus scrape.
///
/// ## Routes Fédérées (Phase 10C)
/// Les routes `/api/v1/repos/:owner/:repo/operations/...` résolvent le
/// couple `(owner, repo)` en `repository_id` via `ResolveRepoUseCase`,
/// puis délèguent aux use cases existants.
pub fn create_router(state: SharedState) -> Router {
    // ── Prometheus Middleware ───────────────────────
    let (prometheus_layer, metric_handle) = axum_prometheus::PrometheusMetricLayer::pair();

    Router::new()
        // Health & status (sans état)
        .route("/health", get(health_check))
        .route("/api/v1/status", get(status))
        // ━━━ Global Routes (cross-repo) ━━━
        .route("/api/v1/chunks/search", get(search_chunks_handler))
        .route(
            "/api/v1/chunks/semantic-search",
            post(semantic_search_handler),
        )
        .route("/api/v1/reviews/scores", get(get_score_history_handler))
        // ━━━ Forge Sociale (Phase 10D — Big Bang) ━━━
        .route("/api/v1/repos", post(create_repository_handler))
        // ━━━ Actors → Repos (Préambule Makimono Phase 5) ━━━
        .route(
            "/api/v1/actors/{handle}/repos",
            get(list_repositories_handler),
        )
        // ━━━ Repo Detail (Préambule Makimono Phase 5) ━━━
        .route("/api/v1/repos/{owner}/{repo}", get(get_repository_handler))
        // ━━━ Federated Routes (Phase 10C — /repos/:owner/:repo) ━━━
        .route(
            "/api/v1/repos/{owner}/{repo}/operations",
            post(federated_create_operation).get(federated_list_operations),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/operations/{id}",
            get(federated_get_operation),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/operations/{id}/diff",
            get(federated_get_diff),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/operations/{id}/reviews",
            get(federated_get_reviews),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/operations/{id}/ipfs",
            get(federated_get_ipfs),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/operations/{id}/chunks",
            get(federated_get_chunks),
        )
        // ── Phase 17 — Diff Colorisé (line-by-line) ─────────────────
        .route(
            "/api/v1/repos/{owner}/{repo}/operations/{id}/diff-content",
            get(federated_get_diff_content),
        )
        // ── Phase 6 — Explorateur de Code (lecture seule) ───────────────
        .route(
            "/api/v1/repos/{owner}/{repo}/tree/{revision}",
            get(explorer_tree_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/refs",
            get(explorer_refs_handler),
        )
        // ── Phase 15 — Sensei Chat IA (SSE streaming) ───────────────
        .route("/api/v1/sensei/chat", post(sensei_chat_handler))
        .route("/api/v1/sensei/models", get(sensei_models_handler))
        .route("/api/v1/sensei/warmup", post(sensei_warmup_handler))
        // ── Phase 19A — Auth & RBAC ─────────────────────────────────
        .route("/api/v1/auth/register", post(crate::rest::auth_routes::register_handler))
        .route("/api/v1/auth/login", post(crate::rest::auth_routes::login_handler))
        .route("/api/v1/auth/me", get(crate::rest::auth_routes::me_handler))
        .route("/api/v1/auth/tokens",
            post(crate::rest::auth_routes::create_pat_handler)
                .get(crate::rest::auth_routes::list_pats_handler),
        )
        // ── Phase 19B — GitHub Import ─────────────────────────────
        .route("/api/v1/repos/import-github", post(import_github_handler))
        .route("/api/v1/github/preview", get(github_preview_handler))
        // ── Phase 20 — GitHub OAuth ──────────────────────────────
        .route("/api/v1/auth/github", get(crate::rest::auth_routes::github_auth_url_handler))
        .route("/api/v1/auth/github/callback", post(crate::rest::auth_routes::github_callback_handler))
        // ── Phase 20B — Le Clonage Massif ────────────────────────
        .route("/api/v1/github/my-repos", get(list_github_repos_handler))
        .route("/api/v1/github/bulk-import", post(bulk_import_github_handler))
        // ── Phase 24 — Soft Delete (Corbeille) ─────────────────
        .route(
            "/api/v1/repos/{owner}/{repo}/archive",
            post(archive_repository_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/restore",
            post(restore_repository_handler),
        )
        .route(
            "/api/v1/actors/{handle}/trash",
            get(list_trash_handler),
        )
        // ── Phase 25 — Service Accounts (L'Acte de Naissance) ────────
        .route(
            "/api/v1/auth/service-accounts",
            post(crate::rest::auth_routes::create_service_account_handler)
                .get(crate::rest::auth_routes::list_service_accounts_handler),
        )
        .route(
            "/api/v1/auth/service-accounts/{id}",
            axum::routing::delete(crate::rest::auth_routes::delete_service_account_handler),
        )
        // ── Phase 25B — Profil Public Acteur ─────────────────────
        .route(
            "/api/v1/actors/{handle}/profile",
            get(actor_profile_handler),
        )
        // ── Phase 26A — Merge Requests (Le Katana Croisé) ─────────
        .route(
            "/api/v1/repos/{owner}/{repo}/mrs",
            post(create_mr_handler).get(list_mrs_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/mrs/{number}",
            get(get_mr_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/mrs/{number}/reviews",
            post(review_mr_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/mrs/{number}/merge",
            post(merge_mr_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/mrs/{number}/close",
            post(close_mr_handler),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/mrs/{number}/diff",
            get(mr_diff_handler),
        )
        // ── Phase 26B — Pré-diff entre branches (formulaire New MR) ────
        .route(
            "/api/v1/repos/{owner}/{repo}/diff-between",
            get(diff_between_handler),
        )
        // ── Métriques Prometheus ────────────────────
        .route(
            "/metrics",
            get(move || async move { metric_handle.render() }),
        )
        .with_state(state)
        // Le layer doit être appliqué APRÈS .with_state() pour couvrir toutes les routes
        .layer(prometheus_layer)
}

// ─── Handlers ────────────────────────────────────

/// Health check — `GET /health`
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "operational".to_string(),
        service: "taijutsu".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Status — `GET /api/v1/status`
async fn status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "taijutsu",
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "vcs_engine": "jujutsu (ACL)",
            "protocol": "ninpo (gRPC)",
            "persistence": "fūinjutsu (PostgreSQL + Redis)",
            "tensai": "semantic chunking (Tree-sitter + ChunkRepository)",
        },
        "status": "operational"
    }))
}

// ─── Handlers Globaux (cross-repo) ────────────────

/// Rechercher des symboles par nom — `GET /api/v1/chunks/search?name=User`
async fn search_chunks_handler(
    State(state): State<SharedState>,
    Query(params): Query<SearchChunksQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        name = %params.name,
        "REST: SearchChunks reçu (Tensai)"
    );

    let filter = ChunkSearchFilter::ByName {
        name: params.name.clone(),
    };

    let result = state.search_chunks.execute(filter).await?;
    let chunks_json: Vec<ChunkJson> = result.chunks.into_iter().map(ChunkJson::from).collect();

    Ok(Json(serde_json::json!({
        "query": params.name,
        "chunks": chunks_json,
        "count": result.count,
    })))
}

/// Recherche sémantique RAG — `POST /api/v1/chunks/semantic-search`
///
/// Transforme la requête en langage naturel en embedding vectoriel,
/// puis exécute une recherche par similarité cosinus via pgvector.
async fn semantic_search_handler(
    State(state): State<SharedState>,
    Json(body): Json<SemanticSearchBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        query = %body.query,
        limit = body.limit,
        threshold = body.threshold,
        "REST: SemanticSearch reçu (Tensai RAG)"
    );

    let result = state
        .search_chunks
        .execute_semantic(&body.query, body.limit, body.threshold)
        .await?;

    let chunks_json: Vec<SemanticChunkJson> = result
        .chunks
        .into_iter()
        .map(SemanticChunkJson::from)
        .collect();

    Ok(Json(serde_json::json!({
        "query": body.query,
        "chunks": chunks_json,
        "count": result.count,
    })))
}

// ─── Handler Score History (Phase 9.2) ───────────────

/// Paramètres de query pour GET /api/v1/reviews/scores.
#[derive(Debug, Deserialize)]
pub struct ScoreHistoryQuery {
    /// Nombre de scores à récupérer (défaut: 10).
    #[serde(default = "default_score_limit")]
    pub limit: usize,
}

fn default_score_limit() -> usize {
    10
}

/// Récupérer l'historique des scores Oracle — `GET /api/v1/reviews/scores?limit=10`
async fn get_score_history_handler(
    State(state): State<SharedState>,
    Query(params): Query<ScoreHistoryQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        limit = params.limit,
        "REST: GetScoreHistory reçu (Sparkline)"
    );

    let result = state.get_score_history.execute(params.limit).await?;

    Ok(Json(serde_json::json!({
        "scores": result.scores,
        "count": result.count,
        "average": result.average,
        "trend": result.trend,
    })))
}

// ── Helpers ───────────────────────────────────────────────────────

/// Décode une chaîne base64 (RFC 4648) en bytes bruts.
///
/// Implémentation inline — cohérente avec l'encodeur dans `create_operation.rs`.
/// Évite l'ajout du crate `base64` pour une seule fonction.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    const DECODE_TABLE: [u8; 128] = {
        let mut table = [255u8; 128];
        let mut i = 0u8;
        while i < 26 {
            table[(b'A' + i) as usize] = i;
            table[(b'a' + i) as usize] = i + 26;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            table[(b'0' + d) as usize] = d + 52;
            d += 1;
        }
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
        table
    };

    let input = input.trim();
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();

    for chunk in bytes.chunks(4) {
        let mut buf = [0u32; 4];
        for (i, &b) in chunk.iter().enumerate() {
            if b >= 128 || DECODE_TABLE[b as usize] == 255 {
                return Err(format!("Invalid base64 character: {}", b as char));
            }
            buf[i] = DECODE_TABLE[b as usize] as u32;
        }
        let triple = (buf[0] << 18) | (buf[1] << 12) | (buf[2] << 6) | buf[3];
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk.len() > 2 {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 {
            result.push((triple & 0xFF) as u8);
        }
    }

    Ok(result)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ─── Phase 10C — Handlers Fédérés (/api/v1/repos/:owner/:repo)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Phase 10D — Forge Sociale (POST /api/v1/repos)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Corps de la requête POST /api/v1/repos.
///
/// 🔒 `owner_id` n'est plus dans le body — il est extrait du JWT.
#[derive(Debug, Deserialize)]
pub struct CreateRepoBody {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// "public" ou "private" (défaut: "public").
    #[serde(default = "default_visibility")]
    pub visibility: String,
}

fn default_visibility() -> String {
    "public".to_string()
}

/// Réponse JSON pour un dépôt créé.
#[derive(Debug, Serialize)]
pub struct RepositoryJson {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub default_branch: String,
    pub created_at: DateTime<Utc>,
    /// URL Git source (non-null = importé depuis GitHub). Phase 19B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_source_url: Option<String>,
    /// Timestamp du dernier import miroir. Phase 19B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror_synced_at: Option<DateTime<Utc>>,
}

impl From<domain::entities::repository::Repository> for RepositoryJson {
    fn from(repo: domain::entities::repository::Repository) -> Self {
        Self {
            id: repo.id,
            owner_id: repo.owner_id,
            name: repo.name,
            display_name: repo.display_name,
            description: repo.description,
            visibility: repo.visibility.as_sql_str().to_string(),
            default_branch: repo.default_branch,
            created_at: repo.created_at,
            mirror_source_url: repo.mirror_source_url,
            mirror_synced_at: repo.mirror_synced_at,
        }
    }
}

/// Créer un dépôt — `POST /api/v1/repos`
///
/// 🔒 **Authentification obligatoire** — le `owner_id` est extrait du JWT,
/// jamais du body client (prévient l'usurpation d'identité).
async fn create_repository_handler(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(body): Json<CreateRepoBody>,
) -> Result<(axum::http::StatusCode, Json<RepositoryJson>), AppError> {
    let owner_id = auth.0.actor_id();

    info!(
        owner_id = %owner_id,
        name = %body.name,
        "REST: CreateRepository reçu (Forge Sociale — Auth)"
    );

    let visibility = Visibility::from_sql_str(&body.visibility).unwrap_or(Visibility::Public);

    let cmd = CreateRepositoryCommand {
        owner_id,
        name: body.name,
        display_name: body.display_name,
        description: body.description,
        visibility,
    };

    let repo = state.create_repository.execute(cmd).await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(RepositoryJson::from(repo)),
    ))
}

/// Lister les dépôts d'un acteur — `GET /api/v1/actors/{handle}/repos`
///
/// ## Visibilité (Phase 23)
/// - Si le visiteur est le propriétaire (JWT match) → tous les repos
/// - Sinon (anonyme ou autre utilisateur) → repos publics uniquement
async fn list_repositories_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(handle = %handle, "REST: ListRepositories reçu");

    let repos = state.list_repositories.execute(&handle).await?;

    // Phase 23 : filtrer par visibilité selon l'identité du visiteur
    let filtered_repos: Vec<domain::entities::repository::Repository> = match &auth.0 {
        Some(claims) => {
            // Vérifier si le visiteur est le propriétaire
            let is_owner = repos.first().map_or(false, |r| r.owner_id == claims.actor_id());
            if is_owner {
                repos // Propriétaire voit tout
            } else {
                // Phase 25 : si le visiteur est un AI agent, vérifier aussi via parent_id
                let mut visible = Vec::new();
                let actor_id = claims.actor_id();

                // Récupérer l'acteur pour vérifier l'héritage AI
                let parent_id_opt = if let Ok(Some(actor)) = state.actor_repo.find_by_id(&actor_id).await {
                    if actor.is_ai() { actor.parent_id } else { None }
                } else {
                    None
                };

                // Vérifier si le visiteur (ou son parent) est le propriétaire
                let is_parent_owner = parent_id_opt.map_or(false, |pid| {
                    repos.first().map_or(false, |r| r.owner_id == pid)
                });
                if is_parent_owner {
                    return Ok(Json(serde_json::json!({
                        "owner": handle,
                        "repositories": repos.into_iter().map(RepositoryJson::from).collect::<Vec<_>>(),
                        "count": 0, // will be overridden
                    })));
                }

                for repo in repos {
                    if repo.is_public() {
                        visible.push(repo);
                    } else if state.repo_repo.is_collaborator(&actor_id, &repo.id).await.unwrap_or(false) {
                        visible.push(repo);
                    } else if let Some(pid) = parent_id_opt {
                        // Phase 25 : héritage RBAC du parent
                        if state.repo_repo.is_collaborator(&pid, &repo.id).await.unwrap_or(false) {
                            visible.push(repo);
                        }
                    }
                }
                visible
            }
        }
        None => {
            // Anonyme : publics uniquement (Option A)
            repos.into_iter().filter(|r| r.is_public()).collect()
        }
    };

    let repos_json: Vec<RepositoryJson> = filtered_repos.into_iter().map(RepositoryJson::from).collect();

    Ok(Json(serde_json::json!({
        "owner": handle,
        "repositories": repos_json,
        "count": repos_json.len(),
    })))
}

/// Détail d'un dépôt — `GET /api/v1/repos/{owner}/{repo}`
///
/// ## Visibilité (Phase 23)
/// Retourne 404 si le repo est privé et que le visiteur n'est pas autorisé.
async fn get_repository_handler(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    auth: MaybeAuth,
) -> Result<Json<RepositoryJson>, AppError> {
    info!(owner = %owner, repo = %repo, "REST: GetRepository reçu");

    let repository = resolve_repo_with_access_check(&state, &owner, &repo, &auth).await?;

    Ok(Json(RepositoryJson::from(repository)))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Paramètres de chemin fédérés : `(owner, repo)`.
#[derive(Debug, Deserialize)]
struct RepoPath {
    owner: String,
    repo: String,
}

/// Paramètres de chemin fédérés avec ID d'opération.
#[derive(Debug, Deserialize)]
struct RepoOperationPath {
    owner: String,
    repo: String,
    id: Uuid,
}

// ── Phase 23 : Garde de visibilité centralisé ─────────────────────────

/// Résout un repo et vérifie l'accès en lecture.
///
/// - Repo public → OK pour tous (même anonyme)
/// - Repo privé → nécessite JWT valide + owner/collaborateur
/// - Non autorisé → 404 (pas 403, pour ne pas révéler l'existence)
async fn resolve_repo_with_access_check(
    state: &SharedState,
    owner: &str,
    repo: &str,
    auth: &MaybeAuth,
) -> Result<domain::entities::repository::Repository, AppError> {
    let repository = state.resolve_repo.execute(owner, repo).await?;

    // Repo public : accès libre
    if repository.is_public() {
        return Ok(repository);
    }

    // Repo privé : vérifier l'identité
    match &auth.0 {
        Some(claims) => {
            let actor_id = claims.actor_id();
            if actor_id == repository.owner_id {
                return Ok(repository); // Propriétaire
            }
            if state.repo_repo.is_collaborator(&actor_id, &repository.id).await.unwrap_or(false) {
                return Ok(repository); // Collaborateur
            }

            // Phase 25 : Héritage RBAC — si l'acteur est un AI agent,
            // vérifier les droits de son parent humain.
            if let Ok(Some(actor)) = state.actor_repo.find_by_id(&actor_id).await {
                if actor.is_ai() {
                    if let Some(parent_id) = actor.parent_id {
                        if parent_id == repository.owner_id {
                            return Ok(repository); // Parent est owner
                        }
                        if state.repo_repo.is_collaborator(&parent_id, &repository.id).await.unwrap_or(false) {
                            return Ok(repository); // Parent est collaborateur
                        }
                    }
                }
            }

            // Auth OK mais pas autorisé → 404 (ne pas révéler l'existence)
            Err(AppError::from(DomainError::NotFound {
                entity_type: "Repository",
                id: uuid::Uuid::nil(),
            }))
        }
        None => {
            // Anonyme sur repo privé → 404
            Err(AppError::from(DomainError::NotFound {
                entity_type: "Repository",
                id: uuid::Uuid::nil(),
            }))
        }
    }
}

/// Créer une opération — `POST /api/v1/repos/:owner/:repo/operations`
async fn federated_create_operation(
    State(state): State<SharedState>,
    Path(path): Path<RepoPath>,
    auth: MaybeAuth,
    Json(body): Json<CreateOperationBody>,
) -> Result<(axum::http::StatusCode, Json<OperationJson>), AppError> {
    info!(
        owner = %path.owner,
        repo = %path.repo,
        author_id = %body.author_id,
        "REST Fédéré: CreateOperation"
    );

    let repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let files: Vec<(String, Vec<u8>)> = body
        .files
        .iter()
        .filter_map(|f| match decode_base64(&f.content_b64) {
            Ok(bytes) => Some((f.path.clone(), bytes)),
            Err(e) => {
                warn!(path = %f.path, error = %e, "⚠️ Décodage base64 échoué — fichier ignoré");
                None
            }
        })
        .collect();

    let cmd = CreateOperationCommand {
        author_id: body.author_id,
        repository_id: repository.id,
        description: body.description,
        parent_ids: body.parent_ids,
        files,
    };

    let result = state.create_operation.execute(cmd).await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(OperationJson::from(result.operation)),
    ))
}

/// Lister les opérations — `GET /api/v1/repos/:owner/:repo/operations`
async fn federated_list_operations(
    State(state): State<SharedState>,
    Path(path): Path<RepoPath>,
    Query(params): Query<ListOperationsQuery>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        owner = %path.owner,
        repo = %path.repo,
        "REST Fédéré: ListOperations"
    );

    let repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let filter = if let Some(author_id) = params.author_id {
        ListFilter::ByAuthor { author_id }
    } else {
        ListFilter::Recent {
            repo_id: repository.id,
            limit: params.limit.unwrap_or(50),
        }
    };

    let operations = state.list_operations.execute(filter).await?;

    // Phase 17 : total_count absolu via COUNT(*) — indépendant du limit
    let total_count = state.operation_repo.count_by_repo(&repository.id).await.unwrap_or(operations.len() as i64);

    let operations_json: Vec<OperationJson> =
        operations.into_iter().map(OperationJson::from).collect();

    Ok(Json(serde_json::json!({
        "operations": operations_json,
        "count": operations_json.len(),
        "total_count": total_count,
    })))
}

/// Retrouver une opération — `GET /api/v1/repos/:owner/:repo/operations/:id`
async fn federated_get_operation(
    State(state): State<SharedState>,
    Path(path): Path<RepoOperationPath>,
    auth: MaybeAuth,
) -> Result<Json<OperationJson>, AppError> {
    info!(
        owner = %path.owner,
        repo = %path.repo,
        id = %path.id,
        "REST Fédéré: GetOperation"
    );

    let _repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let operation = state.get_operation.execute(path.id).await?;
    Ok(Json(OperationJson::from(operation)))
}

/// Récupérer le diff — `GET /api/v1/repos/:owner/:repo/operations/:id/diff`
async fn federated_get_diff(
    State(state): State<SharedState>,
    Path(path): Path<RepoOperationPath>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let _repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let result = state.get_operation_diff.execute(path.id).await?;

    Ok(Json(serde_json::json!({
        "operation_id": result.operation_id,
        "content_id": result.content_id,
        "changed_files": result.changed_files,
        "count": result.changed_files.len(),
    })))
}

/// Récupérer les reviews — `GET /api/v1/repos/:owner/:repo/operations/:id/reviews`
async fn federated_get_reviews(
    State(state): State<SharedState>,
    Path(path): Path<RepoOperationPath>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let _repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let result = state.get_reviews.execute(path.id).await?;
    let reviews_json: Vec<ReviewJson> = result.reviews.into_iter().map(ReviewJson::from).collect();

    Ok(Json(serde_json::json!({
        "operation_id": path.id,
        "reviews": reviews_json,
        "count": result.count,
    })))
}

/// Récupérer le contenu IPFS — `GET /api/v1/repos/:owner/:repo/operations/:id/ipfs`
async fn federated_get_ipfs(
    State(state): State<SharedState>,
    Path(path): Path<RepoOperationPath>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let _repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let result = state.get_ipfs_content.execute(path.id).await?;

    Ok(Json(serde_json::json!({
        "operation_id": result.operation_id,
        "ipfs_cid": result.ipfs_cid,
        "blob_size": result.blob_size,
        "files": result.files,
        "count": result.files.len(),
    })))
}

/// Récupérer les chunks — `GET /api/v1/repos/:owner/:repo/operations/:id/chunks`
async fn federated_get_chunks(
    State(state): State<SharedState>,
    Path(path): Path<RepoOperationPath>,
    Query(params): Query<ChunksQuery>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let _repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let filter = match params.file {
        Some(file_path) => ChunkSearchFilter::ByFile {
            operation_id: path.id,
            file_path,
        },
        None => ChunkSearchFilter::ByOperation {
            operation_id: path.id,
        },
    };

    let result = state.search_chunks.execute(filter).await?;
    let chunks_json: Vec<ChunkJson> = result.chunks.into_iter().map(ChunkJson::from).collect();

    Ok(Json(serde_json::json!({
        "operation_id": path.id,
        "chunks": chunks_json,
        "count": result.count,
    })))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ─── Phase 17 — Diff Colorisé (line-by-line)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Diff ligne par ligne — `GET /api/v1/repos/:owner/:repo/operations/:id/diff-content`
///
/// Retourne le diff structuré avec hunks, lignes add/remove/context,
/// numéros de ligne et flag too_large pour la protection du DOM.
async fn federated_get_diff_content(
    State(state): State<SharedState>,
    Path(path): Path<RepoOperationPath>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        owner = %path.owner,
        repo = %path.repo,
        id = %path.id,
        "REST Fédéré: GetDiffContent (Phase 17)"
    );

    let repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    // Retrouver l'opération pour obtenir le content_id
    let operation = state.get_operation.execute(path.id).await?;

    // Calculer le diff ligne par ligne via le VcsEngine
    let content_id = domain::entities::content_id::ContentId::new(
        operation.content_id.into_inner(),
    );
    let file_diffs = state
        .vcs_engine
        .diff_content(&repository.id, &content_id)
        .await?;

    // Calculer les stats globales
    let total_additions: u32 = file_diffs.iter().map(|f| f.additions).sum();
    let total_deletions: u32 = file_diffs.iter().map(|f| f.deletions).sum();
    let files_changed = file_diffs.len();

    Ok(Json(serde_json::json!({
        "operation_id": path.id,
        "files": file_diffs,
        "stats": {
            "files_changed": files_changed,
            "additions": total_additions,
            "deletions": total_deletions,
        }
    })))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ─── Phase 6 — Explorateur de Code (lecture seule)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Paramètres de chemin pour l'explorateur.
#[derive(Debug, Deserialize)]
struct RepoRevPath {
    owner: String,
    repo: String,
    revision: String,
}

/// Query string pour les routes explorer : `?path=src/main.rs`
#[derive(Debug, Deserialize)]
struct ExplorerPathQuery {
    /// Chemin relatif à explorer (défaut : racine "")
    #[serde(default)]
    path: String,
}

/// Encode des bytes en base64 (RFC 4648, sans padding strict requis).
fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(TABLE[((n >> 18) & 63) as usize] as char);
        result.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(TABLE[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Explorateur unifié — `GET /api/v1/repos/{owner}/{repo}/tree/{revision}?path=`
///
/// Retourne une réponse JSON discriminante :
/// - `kind: "directory"` → liste des entrées (dossiers + fichiers)
/// - `kind: "file"` → contenu du fichier en base64 + langage Shiki
///
/// Un seul appel suffit au frontend pour décider du composant à afficher.
async fn explorer_tree_handler(
    State(state): State<SharedState>,
    Path(path): Path<RepoRevPath>,
    Query(params): Query<ExplorerPathQuery>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        owner = %path.owner,
        repo = %path.repo,
        revision = %path.revision,
        query_path = %params.path,
        "Phase 6: explorer_tree"
    );

    // Phase 23 : check visibilité
    let _repository = resolve_repo_with_access_check(&state, &path.owner, &path.repo, &auth).await?;

    let file_path = params.path.trim_matches('/').to_string();

    // 1. Tenter le listing de répertoire
    match state
        .get_tree
        .execute(&path.owner, &path.repo, &path.revision, &file_path)
        .await
    {
        Ok(result) => {
            let entries_json: Vec<serde_json::Value> = result
                .entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name,
                        "path": e.path,
                        "kind": if e.kind == EntryKind::Directory { "directory" } else { "file" },
                        "size": e.size,
                    })
                })
                .collect();

            return Ok(Json(serde_json::json!({
                "kind": "directory",
                "revision": result.revision,
                "path": result.path,
                "entries": entries_json,
                "count": entries_json.len(),
            })));
        }
        Err(DomainError::IsFile { .. }) => {
            // 2. C'est un fichier — lire le contenu
            let content = state
                .get_blob
                .execute(&path.owner, &path.repo, &path.revision, &file_path)
                .await?;

            let size = content.len() as u64;
            let content_b64 = encode_base64(&content);
            // Détecter si le contenu est du texte valide UTF-8
            let is_text = std::str::from_utf8(&content).is_ok();
            let language =
                infrastructure::vcs::jujutsu_engine::JujutsuEngine::language_for_path(&file_path);

            return Ok(Json(serde_json::json!({
                "kind": "file",
                "revision": path.revision,
                "path": file_path,
                "size": size,
                "is_text": is_text,
                "language": language,
                "content_b64": content_b64,
            })));
        }
        Err(e) => return Err(AppError::from(e)),
    }
}

/// Références — `GET /api/v1/repos/{owner}/{repo}/refs`
///
/// Retourne les branches et tags séparés pour alimenter le BranchSelector.
async fn explorer_refs_handler(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(owner = %owner, repo = %repo, "Phase 6: explorer_refs");

    // Phase 23 : check visibilité
    let _repository = resolve_repo_with_access_check(&state, &owner, &repo, &auth).await?;

    let refs = state.list_refs.execute(&owner, &repo).await?;

    let branches: Vec<serde_json::Value> = refs
        .iter()
        .filter(|r| r.kind == RefKind::Branch)
        .map(|r| serde_json::json!({ "name": r.name, "target": r.target }))
        .collect();

    let tags: Vec<serde_json::Value> = refs
        .iter()
        .filter(|r| r.kind == RefKind::Tag)
        .map(|r| serde_json::json!({ "name": r.name, "target": r.target }))
        .collect();

    Ok(Json(serde_json::json!({
        "branches": branches,
        "tags": tags,
        "total": refs.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════
//  Phase 15 — Sensei (先生) : Chat IA Streaming (SSE)
// ═══════════════════════════════════════════════════════════════════

/// Corps de la requête `POST /api/v1/sensei/chat`.
#[derive(Debug, Deserialize)]
struct SenseiChatBody {
    /// Question de l'utilisateur.
    query: String,
    /// Chemin du fichier ouvert dans l'explorateur.
    file_path: String,
    /// Contenu du fichier (tronqué à ~3000 chars côté frontend).
    file_content: Option<String>,
    /// Langage du fichier (ex: "rust", "typescript").
    language: Option<String>,
    /// Owner du dépôt (ex: "system").
    owner: String,
    /// Nom du dépôt (ex: "hello-world").
    repo: String,
    /// Historique de la conversation (multi-tour).
    #[serde(default)]
    history: Vec<SenseiHistoryMessage>,
}

/// Message dans l'historique de conversation (sérialisation JSON).
#[derive(Debug, Deserialize)]
struct SenseiHistoryMessage {
    role: String,
    content: String,
}

/// Handler SSE pour l'agent Sensei.
///
/// `POST /api/v1/sensei/chat` → `text/event-stream`
///
/// ## Événements SSE émis
///
/// 1. `data: {"type":"context","sources":[...],"oracle_score":85,"oracle_summary":"..."}`
/// 2. `data: {"type":"token","content":"Cette"}`
/// 3. `data: {"type":"token","content":" fonction"}`
/// 4. ... (un événement par token)
/// 5. `data: {"type":"done","model":"qwen2.5-coder:7b","duration_ms":3200}`
///
/// ## Annulation
/// Le client peut fermer la connexion à tout moment (bouton Stop).
/// Le stream se termine proprement grâce au `mpsc::Sender::is_closed()`.
async fn sensei_chat_handler(
    State(state): State<SharedState>,
    Json(body): Json<SenseiChatBody>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>>, AppError>
{
    info!(
        query = %body.query,
        file_path = %body.file_path,
        owner = %body.owner,
        repo = %body.repo,
        "Phase 15: sensei_chat SSE"
    );

    // Vérifier que Sensei est disponible.
    let sensei = state.sensei_chat.as_ref().ok_or_else(|| {
        AppError::from(DomainError::Internal(
            "Sensei Agent non disponible — Ollama #2 est désactivé ou inaccessible".to_string(),
        ))
    })?;

    // Convertir les messages de l'historique.
    let history: Vec<ChatMessage> = body
        .history
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();

    // Construire la requête Sensei.
    let request = SenseiChatRequest {
        query: body.query,
        file_path: body.file_path,
        file_content: body.file_content,
        language: body.language,
        owner: body.owner,
        repo: body.repo,
        history,
    };

    // Exécuter le pipeline Sensei (RAG + Oracle + LLM streaming).
    let (context, rx) = sensei.execute_stream(request).await?;

    // Convertir le Receiver en Stream SSE.
    let context_event = Event::default().data(
        serde_json::to_string(&serde_json::json!({
            "type": "context",
            "sources": context.sources,
            "oracle_score": context.oracle_score,
            "oracle_summary": context.oracle_summary,
        }))
        .unwrap_or_default(),
    );

    // ── Primer SSE : forcer l'envoi immédiat des headers ──────────
    // Ce premier événement "status" oblige Axum à flusher les headers
    // HTTP 200 + Content-Type: text/event-stream instantanément.
    // Le proxy Next.js voit la connexion vivante et attend patiemment
    // que le modèle LLM se charge (~30s cold start).
    let primer_event = Event::default().data(
        serde_json::to_string(&serde_json::json!({
            "type": "status",
            "message": "loading_model"
        }))
        .unwrap_or_default(),
    );

    // Stream : primer → context → tokens LLM.
    let token_stream =
        tokio_stream::wrappers::ReceiverStream::new(rx).map(|chunk| -> Result<Event, std::convert::Infallible> {
            let data = serde_json::to_string(&chunk).unwrap_or_default();
            Ok(Event::default().data(data))
        });

    // Prépendre le primer et le context au stream de tokens.
    let primer_stream = futures_util::stream::once(async move {
        Ok::<Event, std::convert::Infallible>(primer_event)
    });
    let context_stream = futures_util::stream::once(async move {
        Ok::<Event, std::convert::Infallible>(context_event)
    });

    let full_stream = primer_stream.chain(context_stream).chain(token_stream);

    Ok(Sse::new(full_stream).keep_alive(KeepAlive::default()))
}

// ── Sensei Models & Warmup ───────────────────────────────────────────

/// Réponse de GET /api/v1/sensei/models.
#[derive(Debug, Serialize)]
struct SenseiModelsResponse {
    models: Vec<SenseiModelInfo>,
    active: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SenseiModelInfo {
    name: String,
    size: u64,
}

/// GET /api/v1/sensei/models — Liste les modèles installés sur Ollama #2.
async fn sensei_models_handler(
    State(state): State<SharedState>,
) -> Result<Json<SenseiModelsResponse>, AppError> {
    let ollama_url = state.sensei_ollama_url.as_ref().ok_or_else(|| {
        AppError::from(DomainError::Internal("Sensei non activé".to_string()))
    })?;

    let active = state.sensei_chat.as_ref()
        .map(|s| s.model_name().to_string())
        .unwrap_or_default();

    // Appeler l'API Ollama /api/tags pour lister les modèles.
    let resp = reqwest::get(format!("{ollama_url}/api/tags"))
        .await
        .map_err(|e| AppError::from(DomainError::Internal(format!("Ollama tags: {e}"))))?;

    #[derive(Deserialize)]
    struct OllamaTags { models: Vec<OllamaModel> }
    #[derive(Deserialize)]
    struct OllamaModel { name: String, size: u64 }

    let tags: OllamaTags = resp.json().await
        .map_err(|e| AppError::from(DomainError::Internal(format!("Ollama tags parse: {e}"))))?;

    let models = tags.models.into_iter().map(|m| SenseiModelInfo {
        name: m.name,
        size: m.size,
    }).collect();

    Ok(Json(SenseiModelsResponse { models, active }))
}

/// Corps de POST /api/v1/sensei/warmup.
#[derive(Debug, Deserialize)]
struct SenseiWarmupBody {
    model: String,
}

/// POST /api/v1/sensei/warmup — Pré-charge un modèle dans la RAM d'Ollama.
///
/// Le frontend appelle cette route quand l'utilisateur sélectionne un modèle
/// dans le dropdown. Cela force Ollama à charger le modèle en arrière-plan,
/// éliminant le cold-start (~30s) lors du premier message chat.
async fn sensei_warmup_handler(
    State(state): State<SharedState>,
    Json(body): Json<SenseiWarmupBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ollama_url = state.sensei_ollama_url.as_ref().ok_or_else(|| {
        AppError::from(DomainError::Internal("Sensei non activé".to_string()))
    })?;

    info!(model = %body.model, "🥷 Sensei — Warmup modèle demandé");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::from(DomainError::Internal(format!("HTTP client: {e}"))))?;

    // Envoyer un prompt vide avec keep_alive pour forcer le chargement.
    let resp = client
        .post(format!("{ollama_url}/api/generate"))
        .json(&serde_json::json!({
            "model": body.model,
            "prompt": "",
            "keep_alive": "24h"
        }))
        .send()
        .await
        .map_err(|e| AppError::from(DomainError::Internal(format!("Warmup failed: {e}"))))?;

    if resp.status().is_success() {
        info!(model = %body.model, "🥷 Sensei — Modèle chargé en RAM ✅");
        Ok(Json(serde_json::json!({
            "status": "ok",
            "model": body.model,
            "message": "Modèle chargé en RAM"
        })))
    } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(AppError::from(DomainError::Internal(
            format!("Warmup error (HTTP {status}): {text}")
        )))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Phase 19B — GitHub Import (Le Pont des Mondes)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Corps de la requête POST /api/v1/repos/import-github.
#[derive(Debug, Deserialize)]
pub struct ImportGitHubBody {
    /// URL du dépôt GitHub (ex: "https://github.com/tokio-rs/tokio").
    pub github_url: String,
    /// Override du nom de repo dans SHINOBI (défaut: nom GitHub).
    #[serde(default)]
    pub name_override: Option<String>,
}

/// Paramètres de query pour GET /api/v1/github/preview.
#[derive(Debug, Deserialize)]
pub struct GitHubPreviewQuery {
    /// URL du dépôt GitHub à prévisualiser.
    pub url: String,
}

/// Importer un dépôt GitHub — `POST /api/v1/repos/import-github`
///
/// Authentifié : nécessite un JWT valide (AuthUser).
/// Le owner_id est extrait automatiquement du token.
async fn import_github_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Json(body): Json<ImportGitHubBody>,
) -> Result<(axum::http::StatusCode, Json<RepositoryJson>), AppError> {
    info!(
        actor_id = %auth.0.actor_id(),
        github_url = %body.github_url,
        "REST: ImportGitHub reçu (Phase 19B — Le Pont des Mondes)"
    );

    let cmd = ImportGitHubRepoCommand {
        owner_id: auth.0.actor_id(),
        github_url: body.github_url,
        name_override: body.name_override,
    };

    let repo = state.import_github_repo.execute(cmd).await?;

    info!(
        repo_id = %repo.id,
        repo_name = %repo.name,
        "REST: Import GitHub terminé avec succès 🌉"
    );

    Ok((
        axum::http::StatusCode::CREATED,
        Json(RepositoryJson::from(repo)),
    ))
}

/// Prévisualiser un dépôt GitHub — `GET /api/v1/github/preview?url=...`
///
/// Authentifié : protège le rate limit GitHub (60 req/h par IP).
async fn github_preview_handler(
    _auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Query(params): Query<GitHubPreviewQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        url = %params.url,
        "REST: GitHubPreview reçu (Phase 19B)"
    );

    // Parser l'URL pour extraire owner/repo
    let cleaned = params.url.trim().trim_end_matches('/').trim_end_matches(".git");
    let path = if cleaned.contains("github.com") {
        cleaned.split("github.com").last().unwrap_or("").trim_start_matches('/')
    } else {
        cleaned
    };
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.len() < 2 {
        return Err(AppError::from(DomainError::BusinessRule(format!(
            "URL GitHub invalide: '{}'. Format attendu: https://github.com/owner/repo",
            params.url
        ))));
    }

    let (owner, repo) = (parts[0], parts[1]);

    let info = state
        .github_service
        .fetch_repo_info(owner, repo)
        .await?;

    Ok(Json(serde_json::json!({
        "full_name": info.full_name,
        "name": info.name,
        "description": info.description,
        "clone_url": info.clone_url,
        "default_branch": info.default_branch,
        "stars": info.stars,
        "forks": info.forks,
        "language": info.language,
        "license": info.license,
        "is_private": info.is_private,
    })))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ── Phase 20B — Le Clonage Massif (GitHub Bulk Import)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Lister les repos GitHub de l'utilisateur — `GET /api/v1/github/my-repos`
///
/// Authentifié : utilise le github_token stocké de l'acteur connecté.
/// Retourne la liste de repos avec un flag `already_imported`.
async fn list_github_repos_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor_id = auth.0.actor_id();
    info!(
        actor_id = %actor_id,
        "REST: ListGitHubRepos (Phase 20B — Le Clonage Massif)"
    );

    let repos = state.list_github_repos.execute(&actor_id).await?;

    Ok(Json(serde_json::json!({
        "repos": repos,
        "count": repos.len(),
    })))
}

/// Corps de la requête POST /api/v1/github/bulk-import.
#[derive(Debug, Deserialize)]
struct BulkImportGitHubBody {
    /// Liste d'URLs GitHub à importer.
    repo_urls: Vec<String>,
}

/// Import massif de repos GitHub — `POST /api/v1/github/bulk-import`
///
/// Authentifié : le owner_id est extrait du JWT.
/// Importe les repos séquentiellement avec status par repo.
async fn bulk_import_github_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Json(body): Json<BulkImportGitHubBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let actor_id = auth.0.actor_id();
    info!(
        actor_id = %actor_id,
        count = body.repo_urls.len(),
        "REST: BulkImportGitHub (Phase 20B — Le Clonage Massif)"
    );

    let cmd = application::use_cases::bulk_import_github::BulkImportCommand {
        owner_id: actor_id,
        repo_urls: body.repo_urls,
    };

    let result = state.bulk_import_github.execute(cmd).await?;

    Ok(Json(serde_json::json!({
        "results": result.results,
        "imported": result.imported,
        "skipped": result.skipped,
        "failed": result.failed,
        "total": result.results.len(),
    })))
}

// ── Phase 24 — Soft Delete (Corbeille) ────────────────────────────

/// Corps de la requête POST /api/v1/repos/{owner}/{repo}/archive.
#[derive(Debug, Deserialize)]
struct ArchiveRepoBody {
    /// Mot de confirmation tapé par l'utilisateur.
    confirmation_word: String,
    /// Mot de confirmation attendu (généré côté frontend).
    expected_word: String,
}

/// Mettre un dépôt en corbeille — `POST /api/v1/repos/{owner}/{repo}/archive`
///
/// Auth obligatoire (JWT). Seul le propriétaire peut supprimer.
async fn archive_repository_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    Json(body): Json<ArchiveRepoBody>,
) -> Result<axum::http::StatusCode, AppError> {
    info!(
        owner = %owner,
        repo = %repo,
        actor_id = %auth.0.actor_id(),
        "REST: ArchiveRepository (Phase 24 — Soft Delete)"
    );

    let cmd = application::use_cases::delete_repository::SoftDeleteCommand {
        actor_id: auth.0.actor_id(),
        owner,
        repo,
        confirmation_word: body.confirmation_word,
        expected_word: body.expected_word,
    };

    state.delete_repository.execute_soft_delete(cmd).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Restaurer un dépôt depuis la corbeille — `POST /api/v1/repos/{owner}/{repo}/restore`
///
/// Auth obligatoire (JWT). Seul le propriétaire peut restaurer.
async fn restore_repository_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Json<RepositoryJson>, AppError> {
    info!(
        owner = %owner,
        repo = %repo,
        actor_id = %auth.0.actor_id(),
        "REST: RestoreRepository (Phase 24 — Corbeille)"
    );

    let cmd = application::use_cases::delete_repository::RestoreCommand {
        actor_id: auth.0.actor_id(),
        owner,
        repo,
    };

    let repository = state.delete_repository.execute_restore(cmd).await?;
    Ok(Json(RepositoryJson::from(repository)))
}

/// Lister les dépôts en corbeille — `GET /api/v1/actors/{handle}/trash`
///
/// Auth obligatoire (JWT). Retourne les repos soft-deleted du propriétaire
/// avec le temps restant avant purge définitive.
async fn list_trash_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path(handle): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(
        handle = %handle,
        actor_id = %auth.0.actor_id(),
        "REST: ListTrash (Phase 24 — Corbeille)"
    );

    let repos = state.delete_repository.list_trash(&handle).await?;

    let retention_secs = application::use_cases::purge_trash::TRASH_RETENTION_SECS;

    let trash_json: Vec<serde_json::Value> = repos
        .iter()
        .map(|r| {
            let seconds_left = r.seconds_until_purge(retention_secs).unwrap_or(0);
            serde_json::json!({
                "id": r.id,
                "name": r.name,
                "display_name": r.display_name,
                "description": r.description,
                "visibility": r.visibility.as_sql_str(),
                "deleted_at": r.deleted_at,
                "seconds_until_purge": seconds_left,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "owner": handle,
        "trash": trash_json,
        "count": trash_json.len(),
        "retention_seconds": retention_secs,
    })))
}

// ── Phase 25B — Profil Public Acteur ─────────────────────────────────

/// Profil public d'un acteur — `GET /api/v1/actors/{handle}/profile`
///
/// Retourne les informations publiques d'un acteur (humain ou bot).
/// Pour les bots, inclut le parent_handle.
async fn actor_profile_handler(
    State(state): State<SharedState>,
    Path(handle): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!(handle = %handle, "REST: GetActorProfile");

    // Phase 27-pre : profil spécial pour l'acteur système
    if handle.to_lowercase() == "system" {
        return Ok(Json(serde_json::json!({
            "actor": {
                "id": domain::SYSTEM_ACTOR_ID,
                "handle": "system",
                "display_name": "SHINOBI System",
                "actor_type": "system",
                "bio": "Acteur système interne de la Forge Sociale SHINOBI. Responsable des opérations automatiques, migrations, rattachement des données orphelines et actions fédérées.",
                "avatar_url": null,
                "created_at": null,
            },
            "stats": { "public_repos": 0, "total_repos": 0, "bots_count": 0 },
            "parent": null,
            "is_system": true,
        })));
    }

    // Phase 27-pre : bloquer les autres handles réservés qui n'existent pas
    if domain::entities::actor::is_reserved_handle(&handle) {
        return Err(AppError(DomainError::BusinessRule(
            format!("Le handle '{}' est réservé par le système", handle),
        )));
    }

    let actor = state
        .actor_repo
        .find_by_handle(&handle)
        .await?
        .ok_or_else(|| AppError(DomainError::BusinessRule(
            format!("Acteur '{}' introuvable", handle),
        )))?;

    // Compter les repos publics de cet acteur
    let repos = state.list_repositories.execute(&handle).await.unwrap_or_default();
    let public_repos = repos.iter().filter(|r| r.is_public()).count();

    // Si c'est un bot, récupérer le parent
    let parent_info = if actor.is_ai() {
        if let Some(pid) = actor.parent_id {
            if let Ok(Some(parent)) = state.actor_repo.find_by_id(&pid).await {
                Some(serde_json::json!({
                    "id": parent.id,
                    "handle": parent.handle,
                    "display_name": parent.display_name,
                    "avatar_url": parent.avatar_url,
                }))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Compter les bots si c'est un humain
    let bots_count = if actor.is_human() {
        state.actor_repo.list_service_accounts(&actor.id).await.map(|b| b.len()).unwrap_or(0)
    } else {
        0
    };

    Ok(Json(serde_json::json!({
        "actor": {
            "id": actor.id,
            "handle": actor.handle,
            "display_name": actor.display_name,
            "actor_type": actor.actor_type,
            "avatar_url": actor.avatar_url,
            "bio": actor.bio,
            "created_at": actor.created_at,
        },
        "stats": {
            "public_repos": public_repos,
            "total_repos": repos.len(),
            "bots_count": bots_count,
        },
        "parent": parent_info,
    })))
}

// ── Phase 26A — Merge Requests (Le Katana Croisé) ────────────────────────

/// Requête JSON pour créer une MR.
#[derive(Debug, Deserialize)]
struct CreateMrBody {
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    #[serde(default = "default_target_branch")]
    pub target_branch: String,
}

fn default_target_branch() -> String {
    "main".to_string()
}

/// Query params pour filtrer les MR.
#[derive(Debug, Deserialize)]
struct ListMrsQuery {
    pub status: Option<String>,
    #[serde(default = "default_mr_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_mr_limit() -> usize {
    30
}

/// Requête JSON pour reviewer une MR.
#[derive(Debug, Deserialize)]
struct ReviewMrBody {
    pub verdict: String,
    pub body: Option<String>,
}

/// Requête JSON pour merger une MR.
#[derive(Debug, Deserialize)]
struct MergeMrBody {
    #[serde(default = "default_merge_strategy")]
    pub strategy: String,
}

fn default_merge_strategy() -> String {
    "fast_forward".to_string()
}

/// `POST /api/v1/repos/{owner}/{repo}/mrs` — Créer une MR.
async fn create_mr_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    Json(body): Json<CreateMrBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let cmd = application::use_cases::create_mr::CreateMrCommand {
        author_id: auth.0.actor_id(),
        repository_id: repo_entity.id,
        title: body.title,
        description: body.description,
        source_branch: body.source_branch,
        target_branch: body.target_branch,
    };

    let mr = state.create_mr.execute(cmd).await?;

    Ok(Json(serde_json::json!({
        "id": mr.id,
        "number": mr.number,
        "title": mr.title,
        "description": mr.description,
        "source_branch": mr.source_branch,
        "target_branch": mr.target_branch,
        "status": mr.status,
        "author_id": mr.author_id,
        "created_at": mr.created_at,
    })))
}

/// `GET /api/v1/repos/{owner}/{repo}/mrs` — Lister les MR.
async fn list_mrs_handler(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    _auth: MaybeAuth,
    Query(query): Query<ListMrsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let status = query.status.as_deref().and_then(domain::MrStatus::from_sql_str);

    let (mrs, total) = state
        .list_mrs
        .execute(&repo_entity.id, status, query.limit, query.offset)
        .await?;

    let items: Vec<serde_json::Value> = mrs
        .into_iter()
        .map(|mr| {
            serde_json::json!({
                "id": mr.id,
                "number": mr.number,
                "title": mr.title,
                "source_branch": mr.source_branch,
                "target_branch": mr.target_branch,
                "status": mr.status,
                "author_id": mr.author_id,
                "created_at": mr.created_at,
                "updated_at": mr.updated_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "items": items,
        "total": total,
    })))
}

/// `GET /api/v1/repos/{owner}/{repo}/mrs/{number}` — Détail d'une MR.
async fn get_mr_handler(
    State(state): State<SharedState>,
    Path((owner, repo, number)): Path<(String, String, i32)>,
    _auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let detail = state.get_mr.execute(&repo_entity.id, number).await?;

    let reviews: Vec<serde_json::Value> = detail
        .reviews
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "reviewer_id": r.reviewer_id,
                "verdict": r.verdict,
                "body": r.body,
                "created_at": r.created_at,
            })
        })
        .collect();

    let events: Vec<serde_json::Value> = detail
        .events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "actor_id": e.actor_id,
                "event_type": e.event_type.as_sql_str(),
                "payload": e.payload,
                "created_at": e.created_at,
            })
        })
        .collect();

    let mr = &detail.mr;
    Ok(Json(serde_json::json!({
        "id": mr.id,
        "number": mr.number,
        "title": mr.title,
        "description": mr.description,
        "source_branch": mr.source_branch,
        "target_branch": mr.target_branch,
        "status": mr.status,
        "author_id": mr.author_id,
        "merged_by": mr.merged_by,
        "merged_at": mr.merged_at,
        "closed_at": mr.closed_at,
        "created_at": mr.created_at,
        "updated_at": mr.updated_at,
        "has_conflicts": detail.has_conflicts,
        "reviews": reviews,
        "events": events,
    })))
}

/// `POST /api/v1/repos/{owner}/{repo}/mrs/{number}/reviews` — Reviewer une MR.
async fn review_mr_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, number)): Path<(String, String, i32)>,
    Json(body): Json<ReviewMrBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let verdict = domain::MrVerdict::from_sql_str(&body.verdict).ok_or_else(|| {
        DomainError::BusinessRule(format!(
            "Verdict invalide: '{}'. Attendu: 'approve' ou 'changes_requested'",
            body.verdict
        ))
    })?;

    let cmd = application::use_cases::review_mr::ReviewMrCommand {
        reviewer_id: auth.0.actor_id(),
        repository_id: repo_entity.id,
        mr_number: number,
        verdict,
        body: body.body,
    };

    let review = state.review_mr.execute(cmd).await?;

    Ok(Json(serde_json::json!({
        "id": review.id,
        "mr_id": review.mr_id,
        "reviewer_id": review.reviewer_id,
        "verdict": review.verdict,
        "body": review.body,
        "created_at": review.created_at,
    })))
}

/// `POST /api/v1/repos/{owner}/{repo}/mrs/{number}/merge` — Merger une MR.
async fn merge_mr_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, number)): Path<(String, String, i32)>,
    Json(body): Json<MergeMrBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let strategy = match body.strategy.as_str() {
        "fast_forward" => domain::MergeStrategy::FastForward,
        "squash" => domain::MergeStrategy::Squash,
        other => {
            return Err(AppError(DomainError::BusinessRule(format!(
                "Stratégie de merge invalide: '{}'. Attendu: 'fast_forward' ou 'squash'",
                other
            ))));
        }
    };

    let cmd = application::use_cases::merge_mr::MergeMrCommand {
        actor_id: auth.0.actor_id(),
        repository_id: repo_entity.id,
        mr_number: number,
        strategy,
    };

    let result = state.merge_mr.execute(cmd).await?;

    Ok(Json(serde_json::json!({
        "merge_commit_id": result.merge_commit_id,
        "status": "merged",
    })))
}

/// `POST /api/v1/repos/{owner}/{repo}/mrs/{number}/close` — Fermer une MR.
async fn close_mr_handler(
    auth: crate::rest::auth_middleware::AuthUser,
    State(state): State<SharedState>,
    Path((owner, repo, number)): Path<(String, String, i32)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;
    let actor_id = auth.0.actor_id();

    state
        .close_mr
        .execute(&actor_id, &repo_entity.id, number)
        .await?;

    Ok(Json(serde_json::json!({
        "status": "closed",
    })))
}

/// `GET /api/v1/repos/{owner}/{repo}/mrs/{number}/diff` — Diff d'une MR.
async fn mr_diff_handler(
    State(state): State<SharedState>,
    Path((owner, repo, number)): Path<(String, String, i32)>,
    _auth: MaybeAuth,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let files = state.mr_diff.execute(&repo_entity.id, number).await?;

    let files_json: Vec<serde_json::Value> = files
        .into_iter()
        .map(|f| {
            serde_json::json!({
                "path": f.path,
                "status": f.status,
                "hunks": f.hunks,
                "additions": f.additions,
                "deletions": f.deletions,
                "too_large": f.too_large,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "files": files_json,
        "total_files": files_json.len(),
    })))
}

/// Query params pour le pré-diff entre branches.
#[derive(Debug, Deserialize)]
struct DiffBetweenQuery {
    pub source: String,
    pub target: String,
}

/// `GET /api/v1/repos/{owner}/{repo}/diff-between?source=X&target=Y`
///
/// Calcule le diff merge-base entre deux branches AVANT création d'une MR.
/// Utilisé par le formulaire "New MR" pour prévisualiser les changements.
async fn diff_between_handler(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    _auth: MaybeAuth,
    Query(query): Query<DiffBetweenQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo_entity = state.resolve_repo.execute(&owner, &repo).await?;

    let files = state
        .vcs_engine
        .diff_merge_base(&repo_entity.id, &query.source, &query.target)
        .await?;

    let files_json: Vec<serde_json::Value> = files
        .into_iter()
        .map(|f| {
            serde_json::json!({
                "path": f.path,
                "status": f.status,
                "hunks": f.hunks,
                "additions": f.additions,
                "deletions": f.deletions,
                "too_large": f.too_large,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "files": files_json,
        "total_files": files_json.len(),
    })))
}
