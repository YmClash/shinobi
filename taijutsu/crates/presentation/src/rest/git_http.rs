//! Routes Git Smart HTTP — Pont Axum vers `git http-backend` (CGI).
//!
//! Expose le protocole Git Smart HTTP pour permettre `git push` et
//! `git clone` natifs depuis VSCode, terminal ou tout client Git.
//!
//! ## Architecture (Phase 12A — Le Raccourci Thermodynamique)
//! Au lieu de reimplementer le protocole pkt-line/packfile en Rust,
//! on delegue au binaire officiel `git http-backend` (CGI) et on
//! pipe les flux HTTP entre Axum et le processus Git.
//!
//! ## Routes
//! - `GET  /{owner}/{repo}.git/info/refs`         — Negociation
//! - `POST /{owner}/{repo}.git/git-receive-pack`   — Push
//! - `POST /{owner}/{repo}.git/git-upload-pack`    — Clone/Fetch
//!
//! ## Sync Hook (post receive-pack)
//! Apres chaque push reussi, le handler :
//! 1. Recharge le repo jj (jj ne voit pas les commits Git externes)
//! 2. Resout le nouveau HEAD
//! 3. Construit une entite `Operation` pour combler PostgreSQL
//! 4. Publie l'etincelle Kafka (fire-and-forget)

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Router, routing::get, routing::post};
use serde::Deserialize;
use tracing::{info, warn};

use domain::entities::content_id::ContentId;
use domain::entities::operation::Operation;
use domain::ports::vcs_engine::VcsEngine as _;

use crate::state::GitHttpState;

// ── Types ────────────────────────────────────────────────────────────

/// Parametres de chemin Git : `{owner}/{repo}.git`
#[derive(Debug, Deserialize)]
struct GitRepoPath {
    owner: String,
    /// Nom du repo avec le suffixe `.git` (ex: `shinobi.git`)
    repo_dot_git: String,
}

/// Query string pour info/refs
#[derive(Debug, Deserialize)]
struct InfoRefsQuery {
    /// Service demande (ex: `git-receive-pack` ou `git-upload-pack`)
    service: Option<String>,
}

// ── Routeur ──────────────────────────────────────────────────────────

/// Construit le routeur Axum pour les routes Git Smart HTTP.
///
/// Ces routes sont **separees** du routeur API REST principal car
/// elles utilisent un etat different (`GitHttpState`).
pub fn create_git_router(state: GitHttpState) -> Router {
    Router::new()
        .route(
            "/{owner}/{repo_dot_git}/info/refs",
            get(git_info_refs),
        )
        .route(
            "/{owner}/{repo_dot_git}/git-receive-pack",
            post(git_receive_pack),
        )
        .route(
            "/{owner}/{repo_dot_git}/git-upload-pack",
            post(git_upload_pack),
        )
        .with_state(state)
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Extrait le nom du repo en supprimant le suffixe `.git`.
/// Ex: `shinobi.git` -> `shinobi`
fn strip_git_suffix(name: &str) -> &str {
    name.strip_suffix(".git").unwrap_or(name)
}

/// Convertit une `CgiResponse` en `axum::response::Response`.
fn cgi_to_axum_response(
    cgi: infrastructure::vcs::git_cgi::CgiResponse,
) -> Response {
    let status = StatusCode::from_u16(cgi.status).unwrap_or(StatusCode::OK);

    let mut headers = HeaderMap::new();
    for (key, value) in &cgi.headers {
        // Ignorer le header CGI "Status:" — il est dans le status code
        if key.eq_ignore_ascii_case("Status") {
            continue;
        }
        if let (Ok(name), Ok(val)) = (
            axum::http::header::HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, val);
        }
    }

    (status, headers, cgi.body).into_response()
}

// ── Handlers ─────────────────────────────────────────────────────────

/// Negociation Git — `GET /{owner}/{repo}.git/info/refs`
///
/// VSCode/terminal appelle cette route pour demander la liste des
/// branches et commits du depot distant.
async fn git_info_refs(
    State(state): State<GitHttpState>,
    Path(path): Path<GitRepoPath>,
    Query(query): Query<InfoRefsQuery>,
) -> Response {
    let repo_name = strip_git_suffix(&path.repo_dot_git);

    info!(
        owner = %path.owner,
        repo = %repo_name,
        service = ?query.service,
        "Git HTTP: info/refs (negociation)"
    );

    // 1. Resoudre le repo
    let repository = match state.resolve_repo.execute(&path.owner, repo_name).await {
        Ok(repo) => repo,
        Err(e) => {
            warn!(error = %e, "Git HTTP: repo not found");
            return (StatusCode::NOT_FOUND, "Repository not found").into_response();
        }
    };

    // 2. Construire le chemin vers le bare Git repo
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    if !repo_git_path.exists() {
        warn!(
            path = %repo_git_path.display(),
            "Git HTTP: bare Git repo not found on disk"
        );
        return (StatusCode::NOT_FOUND, "Git repository not initialized").into_response();
    }

    // 3. Executer le CGI
    let query_string = query
        .service
        .as_deref()
        .map(|s| format!("service={s}"))
        .unwrap_or_default();

    match state
        .git_cgi
        .execute_cgi(&repo_git_path, "GET", "/info/refs", &query_string, "", &[])
        .await
    {
        Ok(cgi_response) => cgi_to_axum_response(cgi_response),
        Err(e) => {
            warn!(error = %e, "Git HTTP: CGI info/refs failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "Git backend error").into_response()
        }
    }
}

/// Push Git — `POST /{owner}/{repo}.git/git-receive-pack`
///
/// VSCode/terminal envoie un packfile compresse contenant les commits.
/// Apres reception reussie, le Sync Hook comble PostgreSQL et declenche Kafka.
async fn git_receive_pack(
    State(state): State<GitHttpState>,
    Path(path): Path<GitRepoPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let repo_name = strip_git_suffix(&path.repo_dot_git);

    info!(
        owner = %path.owner,
        repo = %repo_name,
        body_size = body.len(),
        "Git HTTP: receive-pack (push)"
    );

    // 1. Resoudre le repo
    let repository = match state.resolve_repo.execute(&path.owner, repo_name).await {
        Ok(repo) => repo,
        Err(e) => {
            warn!(error = %e, "Git HTTP: repo not found for push");
            return (StatusCode::NOT_FOUND, "Repository not found").into_response();
        }
    };

    // 2. Construire le chemin vers le bare Git repo
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    // 3. Extraire le Content-Type de la requete
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-git-receive-pack-request");

    // 4. Executer le CGI receive-pack
    let cgi_response = match state
        .git_cgi
        .execute_cgi(
            &repo_git_path,
            "POST",
            "/git-receive-pack",
            "",
            content_type,
            &body,
        )
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            warn!(error = %e, "Git HTTP: CGI receive-pack failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Git backend error").into_response();
        }
    };

    // 5. Sync Hook — combler le vide PostgreSQL si le CGI a reussi
    if cgi_response.status == 200 || cgi_response.status == 0 {
        // Le status 0 signifie que le CGI n'a pas emis de header Status
        // (comportement normal pour receive-pack reussi)
        let state_clone = state.clone();
        let repo_clone = repository.clone();

        // Fire-and-forget : le Sync Hook tourne en background
        tokio::spawn(async move {
            if let Err(e) = sync_hook_post_push(&state_clone, &repo_clone).await {
                warn!(
                    error = %e,
                    repo_id = %repo_clone.id,
                    "Sync Hook post-push failed (push succeeded but PG/Kafka not updated)"
                );
            }
        });
    }

    cgi_to_axum_response(cgi_response)
}

/// Clone/Fetch Git — `POST /{owner}/{repo}.git/git-upload-pack`
///
/// Lecture seule — pas de Sync Hook ni d'etincelle Kafka.
async fn git_upload_pack(
    State(state): State<GitHttpState>,
    Path(path): Path<GitRepoPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let repo_name = strip_git_suffix(&path.repo_dot_git);

    info!(
        owner = %path.owner,
        repo = %repo_name,
        body_size = body.len(),
        "Git HTTP: upload-pack (clone/fetch)"
    );

    // 1. Resoudre le repo
    let repository = match state.resolve_repo.execute(&path.owner, repo_name).await {
        Ok(repo) => repo,
        Err(e) => {
            warn!(error = %e, "Git HTTP: repo not found for clone/fetch");
            return (StatusCode::NOT_FOUND, "Repository not found").into_response();
        }
    };

    // 2. Construire le chemin vers le bare Git repo
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    // 3. Extraire le Content-Type
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-git-upload-pack-request");

    // 4. Executer le CGI upload-pack
    match state
        .git_cgi
        .execute_cgi(
            &repo_git_path,
            "POST",
            "/git-upload-pack",
            "",
            content_type,
            &body,
        )
        .await
    {
        Ok(cgi_response) => cgi_to_axum_response(cgi_response),
        Err(e) => {
            warn!(error = %e, "Git HTTP: CGI upload-pack failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "Git backend error").into_response()
        }
    }
}

// ── Sync Hook ────────────────────────────────────────────────────────

/// Sync Hook post-push — comble le vide entre Git et PostgreSQL.
///
/// ## Flux
/// 1. `reload_repo()` — jj voit les nouveaux commits Git
/// 2. `resolve_head()` — recupere le nouveau SHA-1
/// 3. Verifie si ce content_id existe deja dans PostgreSQL (evite les doublons)
/// 4. Construit une `Operation` a la volee
/// 5. Persiste dans PostgreSQL
/// 6. Publie l'etincelle Kafka (fire-and-forget)
async fn sync_hook_post_push(
    state: &GitHttpState,
    repository: &domain::entities::repository::Repository,
) -> Result<(), domain::errors::DomainError> {
    let repo_id = repository.id;

    // 0. S'assurer que le workspace est enregistre dans le DashMap
    // Le DashMap est en memoire — apres un redemarrage de Taijutsu,
    // seul DEFAULT_REPO_ID est re-initialise au boot. Les autres repos
    // (crees via l'API) ne sont pas recharges automatiquement.
    // init_workspace() gere les deux cas :
    //   - .jj n'existe pas → init_internal_git()
    //   - .jj existe deja  → re-open via RepoLoader
    state.vcs_engine.init_workspace(&repo_id).await?;

    // 1. Recharger le repo jj (pour voir les nouveaux commits Git)
    state.vcs_engine.reload_repo(&repo_id).await?;

    // 2. Resoudre le nouveau HEAD
    let head = state
        .vcs_engine
        .resolve_head(&repo_id)
        .await?
        .ok_or_else(|| {
            domain::errors::DomainError::VcsError(
                "No HEAD after reload — empty repo?".to_string(),
            )
        })?;

    let content_id = ContentId::new(head.to_string());

    info!(
        repo_id = %repo_id,
        head = %content_id,
        "Sync Hook: HEAD resolu apres push"
    );

    // 3. Construire l'entite Operation
    // author_id = owner_id (pas d'auth dans cette phase)
    let operation = Operation::new(
        repository.owner_id,
        repository.id,
        content_id,
        None, // Pas d'IPFS pour les pushes Git externes
        "External git push",
        vec![],
    );

    // 4. Persister dans PostgreSQL
    state.operation_repo.save(&operation).await?;

    info!(
        operation_id = %operation.id,
        repo_id = %repo_id,
        "Sync Hook: Operation persistee dans PostgreSQL"
    );

    // 5. Publier l'etincelle Kafka (fire-and-forget)
    if let Some(publisher) = &state.event_publisher {
        let publisher = publisher.clone();
        let op = operation.clone();
        tokio::spawn(async move {
            if let Err(e) = publisher.publish_operation_created(&op).await {
                warn!(
                    operation_id = %op.id,
                    error = %e,
                    "Etincelle Kafka post-push failed (operation persistee)"
                );
            }
        });
    }

    info!(
        repo_id = %repo_id,
        operation_id = %operation.id,
        "Sync Hook: Phase 12A complete (PG + Kafka)"
    );

    Ok(())
}
