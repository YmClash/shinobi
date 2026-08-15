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
//! ## Authentification (Phase 19A-Git + Phase 23 — Les Portes Scellées)
//! - **Push** (`git-receive-pack`) : PAT obligatoire via Basic Auth
//! - **Clone/Fetch** (`git-upload-pack`) : Public (V1 — pas de repos prives)
//! - **info/refs** : Auth conditionnelle (seulement si `service=git-receive-pack`)
//!
//! ## Sync Hook (post receive-pack)
//! Apres chaque push reussi, le handler :
//! 1. Recharge le repo jj (jj ne voit pas les commits Git externes)
//! 2. Resout le nouveau HEAD
//! 3. Construit une entite `Operation` pour combler PostgreSQL
//! 4. Publie l'etincelle Kafka (fire-and-forget)

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Router, routing::get, routing::post};
use base64::Engine;
use futures_util::StreamExt;
use serde::Deserialize;
use tracing::{info, warn};

use domain::entities::actor::Actor;
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

// ── Phase 36 — DEBT-007 : Refs pushées (pkt-line) ────────────────────

/// Ref pushée extraite du header pkt-line de `git-receive-pack`.
///
/// Represente une mise a jour atomique d'une branche ou d'un tag.
/// Le body de receive-pack commence par des lignes pkt-line :
///   `<4 hex len><old_sha SP new_sha SP ref_name NUL capabilities LF>`
///   `0000`  ← flush packet (fin des refs)
///   `PACK...` ← packfile binaire
#[derive(Debug, Clone)]
struct PushedRef {
    /// SHA-1 hex de l'ancien commit (0x40 zeros si nouvelle branche)
    #[allow(dead_code)]
    old_sha: String,
    /// SHA-1 hex du nouveau commit
    new_sha: String,
    /// Nom complet de la ref (ex: "refs/heads/test_merge")
    ref_name: String,
}

impl PushedRef {
    /// `true` si c'est une suppression de branche (new_sha = 000...000)
    fn is_delete(&self) -> bool {
        self.new_sha.chars().all(|c| c == '0')
    }

    /// Extrait le nom court de la branche.
    /// Ex: "refs/heads/test_merge" → Some("test_merge")
    fn branch_name(&self) -> Option<&str> {
        self.ref_name.strip_prefix("refs/heads/")
    }
}

// ── Routeur ──────────────────────────────────────────────────────────

/// Construit le routeur Axum pour les routes Git Smart HTTP.
///
/// Ces routes sont **separees** du routeur API REST principal car
/// elles utilisent un etat different (`GitHttpState`).
pub fn create_git_router(state: GitHttpState) -> Router {
    Router::new()
        .route("/{owner}/{repo_dot_git}/info/refs", get(git_info_refs))
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

// ── Auth Helpers (Phase 19A-Git) ─────────────────────────────────────

/// Retourne une reponse 401 avec le challenge WWW-Authenticate.
///
/// Le client Git recevant ce 401 demandera automatiquement les
/// credentials a l'utilisateur (ou au credential helper configure).
fn challenge_401(message: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        "WWW-Authenticate",
        HeaderValue::from_static("Basic realm=\"shinobi\""),
    );
    (StatusCode::UNAUTHORIZED, headers, message.to_string()).into_response()
}

/// Extrait les credentials Basic Auth du header `Authorization`.
///
/// Format attendu : `Basic base64(username:password)`
/// - **username** : handle de l'acteur (informatif, pas utilise pour le lookup)
/// - **password** : PAT brut (ex: `shb_abc123...`)
fn extract_basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let header = headers.get("authorization")?.to_str().ok()?;
    let encoded = header
        .strip_prefix("Basic ")
        .or_else(|| header.strip_prefix("basic "))?;
    let decoded = String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?,
    )
    .ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Authentifie un PAT via Basic Auth et retourne l'acteur associe.
///
/// ## Flux
/// 1. Extrait le header `Authorization: Basic ...`
/// 2. Decode base64 → `(username, password)`
/// 3. Hash SHA-256 du password (= PAT brut)
/// 4. Reverse lookup en DB : hash → Actor
///
/// ## Erreurs
/// Retourne une `Response` 401 avec challenge si :
/// - Header absent ou mal forme
/// - PAT inconnu en base
async fn authenticate_pat(state: &GitHttpState, headers: &HeaderMap) -> Result<Actor, Response> {
    let (username, password) = extract_basic_auth(headers)
        .ok_or_else(|| challenge_401("Authentication required — use a Personal Access Token"))?;

    // Hash SHA-256 du PAT brut pour lookup
    let pat_hash = state.auth_service.hash_pat_for_lookup(&password);

    // Reverse lookup : hash → actor
    let actor = state
        .actor_repo
        .find_actor_by_credential_hash(&pat_hash, "api_key")
        .await
        .map_err(|e| {
            warn!(error = %e, "Git Auth: erreur DB pendant le lookup PAT");
            (StatusCode::INTERNAL_SERVER_ERROR, "Authentication error").into_response()
        })?
        .ok_or_else(|| {
            warn!(
                username = %username,
                "Git Auth: PAT invalide ou inconnu"
            );
            challenge_401("Invalid Personal Access Token")
        })?;

    info!(
        actor_handle = %actor.handle,
        actor_id = %actor.id,
        "Git Auth: PAT valide — acteur authentifie"
    );

    Ok(actor)
}

/// Tente d'authentifier un PAT sans echouer si absent.
///
/// Retourne `Ok(Some(actor))` si un PAT valide est present,
/// `Ok(None)` si aucun header auth n'est fourni,
/// `Err(response)` si le header est present mais invalide.
///
/// Phase 23 : Pour les routes en lecture (clone/fetch), on tente
/// l'auth de facon non-bloquante. Si le repo est public et qu'il
/// n'y a pas de header auth, on laisse passer.
async fn try_authenticate_pat(
    state: &GitHttpState,
    headers: &HeaderMap,
) -> Result<Option<Actor>, Response> {
    // Pas de header auth → anonyme
    let (username, password) = match extract_basic_auth(headers) {
        Some(creds) => creds,
        None => return Ok(None),
    };

    // Header present → valider le PAT
    let pat_hash = state.auth_service.hash_pat_for_lookup(&password);

    let actor = state
        .actor_repo
        .find_actor_by_credential_hash(&pat_hash, "api_key")
        .await
        .map_err(|e| {
            warn!(error = %e, "Git Auth: erreur DB pendant le lookup PAT");
            (StatusCode::INTERNAL_SERVER_ERROR, "Authentication error").into_response()
        })?
        .ok_or_else(|| {
            warn!(
                username = %username,
                "Git Auth: PAT invalide ou inconnu"
            );
            challenge_401("Invalid Personal Access Token")
        })?;

    info!(
        actor_handle = %actor.handle,
        actor_id = %actor.id,
        "Git Auth: PAT valide — acteur authentifie"
    );

    Ok(Some(actor))
}

/// Verifie si un acteur a le droit de pusher dans un repo.
///
/// V1 : owner OU collaborateur (tout role).
/// Le RBAC granulaire (maintainer vs contributor) viendra en V2.
async fn is_authorized_to_push(
    state: &GitHttpState,
    actor: &Actor,
    repository: &domain::entities::repository::Repository,
) -> bool {
    // L'owner a toujours acces
    if actor.id == repository.owner_id {
        return true;
    }
    // Verifier si collaborateur direct
    if state
        .repo_repo
        .is_collaborator(&actor.id, &repository.id)
        .await
        .unwrap_or(false)
    {
        return true;
    }
    // Phase 25 : Heritage RBAC — si AI agent, verifier les droits du parent
    if actor.is_ai() {
        if let Some(parent_id) = actor.parent_id {
            if parent_id == repository.owner_id {
                return true;
            }
            return state
                .repo_repo
                .is_collaborator(&parent_id, &repository.id)
                .await
                .unwrap_or(false);
        }
    }
    false
}

/// Verifie si un acteur (ou anonyme) a le droit de lire un repo.
///
/// Phase 23 — Les Portes Scellees :
/// - Repo public → toujours OK (meme anonyme)
/// - Repo prive → doit etre owner OU collaborateur
async fn is_authorized_to_read(
    state: &GitHttpState,
    actor: Option<&Actor>,
    repository: &domain::entities::repository::Repository,
) -> bool {
    if repository.is_public() {
        return true;
    }
    // Repo prive : seul le owner ou un collaborateur peut lire
    match actor {
        Some(a) => {
            if a.id == repository.owner_id {
                return true;
            }
            if state
                .repo_repo
                .is_collaborator(&a.id, &repository.id)
                .await
                .unwrap_or(false)
            {
                return true;
            }
            // Phase 25 : Heritage RBAC — si AI agent, verifier les droits du parent
            if a.is_ai() {
                if let Some(parent_id) = a.parent_id {
                    if parent_id == repository.owner_id {
                        return true;
                    }
                    return state
                        .repo_repo
                        .is_collaborator(&parent_id, &repository.id)
                        .await
                        .unwrap_or(false);
                }
            }
            false
        }
        None => false, // Anonyme ne peut pas lire un repo prive
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Extrait le nom du repo en supprimant le suffixe `.git`.
/// Ex: `shinobi.git` -> `shinobi`
fn strip_git_suffix(name: &str) -> &str {
    name.strip_suffix(".git").unwrap_or(name)
}

/// Convertit une `CgiResponse` en `axum::response::Response`.
fn cgi_to_axum_response(cgi: infrastructure::vcs::git_cgi::CgiResponse) -> Response {
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
///
/// ## Auth (Phase 19A-Git + Phase 23)
/// - Si `service=git-receive-pack` (push) : PAT obligatoire
/// - Si `service=git-upload-pack` (clone) :
///   - Repo public : pas de PAT requis
///   - Repo privé : PAT obligatoire + owner/collaborateur
async fn git_info_refs(
    State(state): State<GitHttpState>,
    Path(path): Path<GitRepoPath>,
    Query(query): Query<InfoRefsQuery>,
    headers: HeaderMap,
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

    // 1b. Auth conditionnelle selon service + visibilite
    match query.service.as_deref() {
        // Push : PAT toujours obligatoire (Phase 19A-Git)
        Some("git-receive-pack") => {
            let actor = match authenticate_pat(&state, &headers).await {
                Ok(actor) => actor,
                Err(response) => return response,
            };

            if !is_authorized_to_push(&state, &actor, &repository).await {
                warn!(
                    actor = %actor.handle,
                    repo = %repo_name,
                    owner = %path.owner,
                    "Git Auth: acces refuse — pas owner ni collaborateur"
                );
                return challenge_401("No access to this repository");
            }

            info!(
                actor = %actor.handle,
                repo = %repo_name,
                "Git HTTP: push auth OK (info/refs)"
            );
        }
        // Clone/Fetch : check visibilite (Phase 23)
        Some("git-upload-pack") => {
            let actor = match try_authenticate_pat(&state, &headers).await {
                Ok(maybe_actor) => maybe_actor,
                Err(response) => return response, // PAT present mais invalide
            };

            if !is_authorized_to_read(&state, actor.as_ref(), &repository).await {
                // Repo prive sans auth → challenge 401
                if actor.is_none() {
                    return challenge_401("Authentication required for private repository");
                }
                // Auth OK mais pas autorise → 404 (ne pas reveler l'existence)
                return (StatusCode::NOT_FOUND, "Repository not found").into_response();
            }
        }
        // Autre service ou absent → pas de check supplementaire
        _ => {}
    }

    // 2. Enregistrer le workspace dans le registre DashMap (idempotent)
    //    DOIT être fait AVANT git_repo_path() car celui-ci a besoin de
    //    l'owner_id dans le DashMap pour construire le chemin multi-tenant
    //    {workspace_root}/{owner_id}/{repo_id}/.jj/repo/store/git
    //    Sans cet appel préalable, git_repo_path() utilise le fallback plat
    //    {workspace_root}/{repo_id}/... qui n'existe pas (DEBT-001).
    if let Err(e) = state
        .vcs_engine
        .init_workspace(&repository.owner_id, &repository.id)
        .await
    {
        warn!(error = %e, "Git HTTP: workspace init failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to initialize Git repository",
        )
            .into_response();
    }

    // Maintenant que le DashMap contient owner_id, le chemin est correct
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    if !repo_git_path.exists() {
        warn!(
            repo_id = %repository.id,
            path = %repo_git_path.display(),
            "Git HTTP: bare Git repo absent même après init_workspace"
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
///
/// ## Auth (Phase 19A-Git)
/// PAT obligatoire — toute ecriture est authentifiee.
///
/// ## Phase 36 — Streaming (DEBT-007)
/// Le body n'est plus charge en RAM (`body: Body` au lieu de `body: Bytes`).
/// Les refs pushees sont extraites du header pkt-line (~200 bytes),
/// puis le flux PACK est pipe directement vers `git http-backend` via
/// `execute_cgi_streaming()`. Pic memoire ~ 64 Ko au lieu de la taille
/// totale du push.
async fn git_receive_pack(
    State(state): State<GitHttpState>,
    Path(path): Path<GitRepoPath>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let repo_name = strip_git_suffix(&path.repo_dot_git);

    info!(
        owner = %path.owner,
        repo = %repo_name,
        "Git HTTP: receive-pack (push — streaming)"
    );

    // 0. Auth PAT obligatoire (Phase 19A-Git)
    let actor = match authenticate_pat(&state, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    // 1. Resoudre le repo
    let repository = match state.resolve_repo.execute(&path.owner, repo_name).await {
        Ok(repo) => repo,
        Err(e) => {
            warn!(error = %e, "Git HTTP: repo not found for push");
            return (StatusCode::NOT_FOUND, "Repository not found").into_response();
        }
    };

    // 1b. Verifier l'ownership/collaboration (Phase 19A-Git)
    if !is_authorized_to_push(&state, &actor, &repository).await {
        warn!(
            actor = %actor.handle,
            repo = %repo_name,
            owner = %path.owner,
            "Git Auth: push refuse — pas owner ni collaborateur"
        );
        return challenge_401("No access to this repository");
    }

    info!(
        actor = %actor.handle,
        repo = %repo_name,
        "Git HTTP: push authentifie et autorise"
    );

    // 2. Enregistrer le workspace dans le DashMap (idempotent) avant
    //    git_repo_path() pour le chemin multi-tenant correct (DEBT-001 fix)
    if let Err(e) = state
        .vcs_engine
        .init_workspace(&repository.owner_id, &repository.id)
        .await
    {
        warn!(error = %e, "Git HTTP: workspace init failed for push");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to initialize Git repository",
        )
            .into_response();
    }
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    // 3. Extraire le Content-Type de la requete
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-git-receive-pack-request")
        .to_string();

    // 3b. Relayer CONTENT_LENGTH depuis le header HTTP client (pas de calcul local)
    let content_length = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 4. Phase 36 — Extraire les refs pushees depuis le flux pkt-line (streaming)
    //    Seul le header pkt-line (~200 bytes) est bufferise en RAM.
    //    Le PACK binaire n'est JAMAIS stocke en memoire.
    let (pushed_refs, prefix, remaining_stream) = extract_pushed_refs_from_body(body).await;

    info!(
        ref_count = pushed_refs.len(),
        refs = ?pushed_refs.iter().map(|r| &r.ref_name).collect::<Vec<_>>(),
        "Git HTTP: refs pushees extraites du pkt-line"
    );

    // 5. Executer le CGI receive-pack en streaming (zero copie RAM pour le PACK)
    let cgi_response = match state
        .git_cgi
        .execute_cgi_streaming(
            &repo_git_path,
            "/git-receive-pack",
            &content_type,
            content_length,
            prefix,
            remaining_stream,
        )
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            warn!(error = %e, "Git HTTP: CGI receive-pack (streaming) failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Git backend error").into_response();
        }
    };

    // 6. Sync Hook — combler le vide PostgreSQL si le CGI a reussi
    if cgi_response.status == 200 || cgi_response.status == 0 {
        // Le status 0 signifie que le CGI n'a pas emis de header Status
        // (comportement normal pour receive-pack reussi)
        let state_clone = state.clone();
        let repo_clone = repository.clone();

        // Fire-and-forget : le Sync Hook tourne en background
        // Phase 36 : on passe les pushed_refs au lieu de resolve_git_head()
        tokio::spawn(async move {
            if let Err(e) = sync_hook_post_push(&state_clone, &repo_clone, pushed_refs).await {
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
///
/// ## Auth (Phase 23 — Les Portes Scellees)
/// - Repo public : acces libre (aucun PAT requis)
/// - Repo prive : PAT obligatoire + owner/collaborateur
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

    // 2. Phase 23 : check visibilite
    let actor = match try_authenticate_pat(&state, &headers).await {
        Ok(maybe_actor) => maybe_actor,
        Err(response) => return response, // PAT present mais invalide
    };

    if !is_authorized_to_read(&state, actor.as_ref(), &repository).await {
        if actor.is_none() {
            return challenge_401("Authentication required for private repository");
        }
        // Auth OK mais pas autorise → 404 (ne pas reveler l'existence)
        return (StatusCode::NOT_FOUND, "Repository not found").into_response();
    }

    // 3. Enregistrer le workspace dans le DashMap (idempotent) avant
    //    git_repo_path() pour le chemin multi-tenant correct (DEBT-001 fix)
    if let Err(e) = state
        .vcs_engine
        .init_workspace(&repository.owner_id, &repository.id)
        .await
    {
        warn!(error = %e, "Git HTTP: workspace init failed for clone/fetch");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to initialize Git repository",
        )
            .into_response();
    }
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    // 4. Extraire le Content-Type
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-git-upload-pack-request");

    // 5. Executer le CGI upload-pack
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

// ── Phase 36 — Streaming pkt-line parser ─────────────────────────────

/// Lit le flux Body d'Axum, extrait les refs pushees du header pkt-line,
/// et retourne le buffer consomme + le flux restant (PACK data).
///
/// ## Protocole pkt-line (Git Smart HTTP)
/// Le body de receive-pack commence par des lignes pkt-line :
///   `<4 hex len><old_sha SP new_sha SP ref_name NUL capabilities LF>`
///   ...
///   `0000`   ← flush packet (fin des refs)
///   `PACK...` ← packfile binaire (potentiellement > 1 Go)
///
/// ## Garanties memoire
/// - Buffer de lecture plafonne a 64 Ko (meme si le header est plus grand,
///   impossible en pratique : 1000 branches × 100 bytes = 100 Ko max)
/// - Le PACK binaire N'EST JAMAIS STOCKE en memoire
///
/// ## Retour
/// `(pushed_refs, prefix_bytes, remaining_stream)` ou :
/// - `pushed_refs` : les refs extraites du pkt-line
/// - `prefix_bytes` : les octets consommes (a reecrire dans stdin CGI)
/// - `remaining_stream` : le flux restant (PACK data) a piper dans stdin CGI
async fn extract_pushed_refs_from_body(
    body: Body,
) -> (
    Vec<PushedRef>,
    Vec<u8>,
    std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<Vec<u8>, String>> + Send>>,
) {
    let mut stream = body.into_data_stream();
    let mut buf = Vec::new();
    let mut found_flush = false;

    // Lire les chunks jusqu'au flush packet 0000 ou cap 64 Ko
    const MAX_HEADER_SIZE: usize = 65_536;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                buf.extend_from_slice(&chunk);

                // Chercher le flush packet "0000" dans le buffer
                if buf.windows(4).any(|w| w == b"0000") {
                    found_flush = true;
                    break;
                }

                // Safety cap — le header pkt-line ne devrait jamais depasser 64 Ko
                if buf.len() > MAX_HEADER_SIZE {
                    warn!(
                        buf_size = buf.len(),
                        "extract_pushed_refs: header pkt-line > 64 Ko — arret de la lecture"
                    );
                    break;
                }
            }
            Err(e) => {
                warn!(error = %e, "extract_pushed_refs: erreur lecture body stream");
                break;
            }
        }
    }

    // Parser les refs depuis le buffer
    let refs = if found_flush {
        parse_pktline_refs(&buf)
    } else {
        warn!("extract_pushed_refs: flush packet 0000 non trouve — refs vides");
        Vec::new()
    };

    info!(
        ref_count = refs.len(),
        buf_size = buf.len(),
        found_flush = found_flush,
        "extract_pushed_refs: pkt-line header parse"
    );

    // Convertir le stream restant en Pin<Box<dyn Stream>> pour le CGI
    let remaining = Box::pin(stream.map(|result| {
        result
            .map(|bytes| bytes.to_vec())
            .map_err(|e| e.to_string())
    }));

    (refs, buf, remaining)
}

/// Parse les lignes pkt-line d'un buffer brut et extrait les `PushedRef`.
///
/// ## Format pkt-line
/// Chaque ligne commence par 4 caracteres hex = longueur totale de la ligne
/// (y compris les 4 caracteres de longueur eux-memes).
/// Le contenu est : `<old_sha> <new_sha> <ref_name>[\0capabilities]\n`
///
/// Le flush packet `0000` signale la fin des refs.
fn parse_pktline_refs(buf: &[u8]) -> Vec<PushedRef> {
    let mut refs = Vec::new();
    let mut pos = 0;

    while pos + 4 <= buf.len() {
        // Lire la longueur pkt-line (4 hex chars)
        let len_hex = match std::str::from_utf8(&buf[pos..pos + 4]) {
            Ok(s) => s,
            Err(_) => break,
        };

        // Flush packet = fin des refs
        if len_hex == "0000" {
            break;
        }

        // Decoder la longueur
        let pkt_len = match u16::from_str_radix(len_hex, 16) {
            Ok(n) => n as usize,
            Err(_) => break,
        };

        // Longueur 0 ou < 4 = invalide
        if pkt_len < 4 {
            break;
        }

        // Verifier qu'on a assez de bytes
        if pos + pkt_len > buf.len() {
            warn!(
                pos = pos,
                pkt_len = pkt_len,
                buf_len = buf.len(),
                "parse_pktline_refs: pkt-line tronquee"
            );
            break;
        }

        // Extraire le contenu de la ligne (sans les 4 chars de longueur)
        let line_bytes = &buf[pos + 4..pos + pkt_len];
        let line_str = String::from_utf8_lossy(line_bytes);

        // Supprimer les capabilities apres \0 et le \n final
        let payload = line_str
            .split('\0')
            .next()
            .unwrap_or("")
            .trim_end_matches('\n')
            .trim_end_matches('\r');

        // Parser : "<old_sha> <new_sha> <ref_name>"
        let parts: Vec<&str> = payload.splitn(3, ' ').collect();
        if parts.len() == 3 {
            let old_sha = parts[0].to_string();
            let new_sha = parts[1].to_string();
            let ref_name = parts[2].to_string();

            // Valider que les SHA sont bien des hex de 40 chars
            if old_sha.len() == 40 && new_sha.len() == 40 {
                refs.push(PushedRef {
                    old_sha,
                    new_sha,
                    ref_name,
                });
            }
        }

        pos += pkt_len;
    }

    refs
}

/// Sync Hook post-push — comble le vide entre Git et PostgreSQL.
///
/// ## Flux (Phase 36 — Multi-Branch, enrichi)
/// 1. `reload_repo()` — jj voit les nouveaux commits Git
/// 2. Pour chaque `PushedRef` (branche pushee) :
///    a. `content_id` = `pushed_ref.new_sha` (direct, plus de resolve_git_head)
///    b. `read_commit_snapshot()` — lit les fichiers + description
///    c. `store_dag()` — stocke les fichiers sur IPFS (graceful degradation)
///    d. Resout les parents (lignee sanguine)
///    e. Construit + persiste l'`Operation` dans PostgreSQL
///    f. Publie l'etincelle Kafka
///    g. Publie l'activite federee Push avec le vrai nom de branche
/// 3. Fallback : si `pushed_refs` est vide, retombe sur `resolve_git_head()`
async fn sync_hook_post_push(
    state: &GitHttpState,
    repository: &domain::entities::repository::Repository,
    pushed_refs: Vec<PushedRef>,
) -> Result<(), domain::errors::DomainError> {
    let repo_id = repository.id;

    // 0. S'assurer que le workspace est enregistre dans le DashMap
    state
        .vcs_engine
        .init_workspace(&repository.owner_id, &repo_id)
        .await?;

    // 1. Recharger le repo jj (pour voir les nouveaux commits Git)
    state.vcs_engine.reload_repo(&repo_id).await?;

    // 2. Collecter les refs a traiter (filtrer deletes + non-branches)
    let refs_to_process: Vec<&PushedRef> = pushed_refs
        .iter()
        .filter(|r| !r.is_delete() && r.ref_name.starts_with("refs/heads/"))
        .collect();

    // 3. Fallback : si aucune ref extractible, utiliser resolve_git_head()
    //    (ne devrait pas arriver, mais securite pour les cas limites)
    if refs_to_process.is_empty() {
        warn!(
            repo_id = %repo_id,
            pushed_refs_count = pushed_refs.len(),
            "Sync Hook: aucune ref de branche extractible — fallback resolve_git_head()"
        );
        let content_id = state.vcs_engine.resolve_git_head(&repo_id).ok_or_else(|| {
            domain::errors::DomainError::VcsError(
                "No git HEAD after reload — empty repo?".to_string(),
            )
        })?;
        return sync_hook_process_single_ref(state, repository, &content_id, "main").await;
    }

    // 4. Traiter chaque ref pushee sequentiellement
    //    (sequentiel pour maintenir la lignee parentale en PG)
    for pushed_ref in &refs_to_process {
        let branch_name = pushed_ref.branch_name().unwrap_or("unknown");
        let content_id = domain::entities::content_id::ContentId::new(
            pushed_ref.new_sha.clone(),
        );

        info!(
            repo_id = %repo_id,
            branch = %branch_name,
            content_id = %content_id,
            "Sync Hook: traitement ref pushee"
        );

        if let Err(e) = sync_hook_process_single_ref(
            state, repository, &content_id, branch_name,
        ).await {
            warn!(
                error = %e,
                repo_id = %repo_id,
                branch = %branch_name,
                content_id = %content_id,
                "Sync Hook: erreur traitement ref — continue avec les suivantes"
            );
            // Continue avec les autres refs au lieu d'abandonner tout
        }
    }

    info!(
        repo_id = %repo_id,
        refs_processed = refs_to_process.len(),
        "Sync Hook: Phase 36 complete — toutes les refs traitees"
    );

    Ok(())
}

/// Traite une seule ref pushee : snapshot + IPFS + PG + Kafka + Federation.
///
/// Extraite de `sync_hook_post_push()` pour etre reutilisable dans la
/// boucle multi-ref ET dans le fallback resolve_git_head().
async fn sync_hook_process_single_ref(
    state: &GitHttpState,
    repository: &domain::entities::repository::Repository,
    content_id: &domain::entities::content_id::ContentId,
    branch_name: &str,
) -> Result<(), domain::errors::DomainError> {
    let repo_id = repository.id;

    // 1. Lire le snapshot du commit (fichiers + description)
    let snapshot = state
        .vcs_engine
        .read_commit_snapshot(&repo_id, content_id)
        .await?;

    let description = if snapshot.description.trim().is_empty() {
        "External git push".to_string()
    } else {
        snapshot.description.trim().to_string()
    };

    info!(
        repo_id = %repo_id,
        branch = %branch_name,
        description = %description.chars().take(80).collect::<String>(),
        file_count = snapshot.files.len(),
        "Sync Hook: snapshot lu (fichiers + description)"
    );

    // 2. Stocker les fichiers sur IPFS via store_dag (graceful degradation)
    let ipfs_cid = if let Some(store) = &state.content_store {
        if !snapshot.files.is_empty() {
            match store.store_dag(&description, &snapshot.files).await {
                Ok((root_cid, manifest)) => {
                    info!(
                        ipfs_cid = %root_cid,
                        file_count = manifest.files.len(),
                        "Sync Hook: Merkle DAG stocke sur IPFS"
                    );

                    // Pin en background (fire-and-forget)
                    let store_clone = store.clone();
                    let cid_clone = root_cid.clone();
                    tokio::spawn(async move {
                        if let Err(e) = store_clone.pin(&cid_clone).await {
                            warn!(
                                ipfs_cid = %cid_clone,
                                error = %e,
                                "Sync Hook: pin IPFS echoue (contenu non epingle)"
                            );
                        }
                    });

                    Some(root_cid)
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "Sync Hook: stockage IPFS echoue — operation continue sans CID IPFS"
                    );
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // 3. Résoudre les parents Git → UUID Operation (La Lignée Sanguine — Phase 12A-Fix2)
    let mut parent_op_ids: Vec<uuid::Uuid> = Vec::new();
    for parent_sha in &snapshot.parent_commit_ids {
        match state
            .operation_repo
            .find_by_content_id(&repo_id, parent_sha)
            .await
        {
            Ok(Some(parent_op)) => {
                parent_op_ids.push(parent_op.id);
                info!(
                    parent_sha = %parent_sha,
                    parent_op_id = %parent_op.id,
                    "Sync Hook: parent resolu (SHA-1 → UUID)"
                );
            }
            Ok(None) => {
                warn!(
                    parent_sha = %parent_sha,
                    repo_id = %repo_id,
                    "Sync Hook: parent SHA-1 introuvable en base — orphelin accepte"
                );
            }
            Err(e) => {
                warn!(
                    parent_sha = %parent_sha,
                    error = %e,
                    "Sync Hook: erreur resolution parent — skip"
                );
            }
        }
    }

    info!(
        repo_id = %repo_id,
        parent_count = parent_op_ids.len(),
        total_git_parents = snapshot.parent_commit_ids.len(),
        "Sync Hook: lignee parentale resolue"
    );

    // 4. Construire l'entite Operation complete
    let operation = Operation::new(
        repository.owner_id,
        repository.id,
        content_id.clone(),
        ipfs_cid,
        &description,
        parent_op_ids,
    );

    // 5. Persister dans PostgreSQL (avec detection de doublon)
    //    Si le meme content_id existe deja (push idempotent ou re-push),
    //    on skip silencieusement au lieu de crasher.
    match state.operation_repo.save(&operation).await {
        Ok(()) => {
            info!(
                operation_id = %operation.id,
                repo_id = %repo_id,
                branch = %branch_name,
                has_ipfs = operation.has_ipfs_content(),
                file_count = snapshot.files.len(),
                "Sync Hook: Operation persistee dans PostgreSQL"
            );
        }
        Err(domain::errors::DomainError::Duplicate(msg)) => {
            info!(
                repo_id = %repo_id,
                content_id = %operation.content_id,
                msg = %msg,
                "Sync Hook: content_id deja present (push idempotent) — skip"
            );
            return Ok(());
        }
        Err(e) => {
            // Verifier si c'est une erreur de contrainte unique deguisee
            let err_str = e.to_string();
            if err_str.contains("duplicate key") || err_str.contains("unique constraint") {
                info!(
                    repo_id = %repo_id,
                    content_id = %operation.content_id,
                    "Sync Hook: content_id deja present (contrainte unique) — skip"
                );
                return Ok(());
            }
            return Err(e);
        }
    }

    // 6. Publier l'etincelle Kafka (fire-and-forget)
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

    // 7. Publier l'activite federee Push (Phase 27-ter — fire-and-forget)
    //    Phase 36 : utilise le vrai nom de branche au lieu de "main" hardcode
    if let Some(federation) = &state.federation_service {
        // Resoudre le handle du owner pour les URIs AP
        let owner_handle = match state.actor_repo.find_by_id(&repository.owner_id).await {
            Ok(Some(actor)) => actor.handle,
            _ => {
                warn!(
                    repo_id = %repo_id,
                    "Sync Hook: cannot resolve owner handle for federation — skipping Push activity"
                );
                return Ok(());
            }
        };

        let commit_info = infrastructure::federation::activity_builder::CommitInfo {
            sha: operation.content_id.to_string(),
            message: description.clone(),
        };

        let activity = infrastructure::federation::activity_builder::push_activity(
            &state.federation_domain,
            &owner_handle,
            repository,
            branch_name, // Phase 36: vrai nom de branche au lieu de "main"
            &operation.content_id.to_string(),
            &[commit_info],
        );

        let scheme = if state.federation_domain.contains("localhost") { "http" } else { "https" };
        let repo_uri = format!(
            "{}://{}/repos/{}/{}",
            scheme, state.federation_domain, owner_handle, repository.name
        );

        let fed = federation.clone();
        let owner_id = repository.owner_id;

        tokio::spawn(async move {
            if let Err(e) = fed
                .publish_activity(&owner_id, "Push", "Repository", &repo_uri, activity)
                .await
            {
                warn!(
                    error = %e,
                    "Sync Hook: Federation Push fanout failed (non-fatal)"
                );
            }
        });

        info!(
            repo_id = %repo_id,
            branch = %branch_name,
            "📤 Federation: Push activity queued for fanout"
        );
    }

    info!(
        repo_id = %repo_id,
        branch = %branch_name,
        operation_id = %operation.id,
        "Sync Hook: ref complete (PG + IPFS + Kafka + Federation)"
    );

    Ok(())
}

// ── Tests Phase 36 — DEBT-007 ────────────────────────────────────────

#[cfg(test)]
mod tests_pktline {
    use super::*;

    /// Helper: construire une ligne pkt-line à partir de son contenu.
    /// Le format est : 4 hex chars (longueur totale) + contenu
    fn make_pktline(content: &str) -> Vec<u8> {
        let len = content.len() + 4; // +4 pour les chars de longueur eux-memes
        format!("{:04x}{}", len, content).into_bytes()
    }

    #[test]
    fn test_parse_pktline_single_ref() {
        let old = "0000000000000000000000000000000000000000";
        let new = "abcdef1234567890abcdef1234567890abcdef12";
        let line = format!("{old} {new} refs/heads/main\n");
        let mut buf = make_pktline(&line);
        buf.extend_from_slice(b"0000");

        let refs = parse_pktline_refs(&buf);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].old_sha, old);
        assert_eq!(refs[0].new_sha, new);
        assert_eq!(refs[0].ref_name, "refs/heads/main");
    }

    #[test]
    fn test_parse_pktline_multi_ref() {
        let sha1 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let sha2 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let sha3 = "cccccccccccccccccccccccccccccccccccccccc";

        let line1 = format!("{sha1} {sha2} refs/heads/main\n");
        let line2 = format!("{sha1} {sha3} refs/heads/feature/test\n");
        let mut buf = make_pktline(&line1);
        buf.extend_from_slice(&make_pktline(&line2));
        buf.extend_from_slice(b"0000");

        let refs = parse_pktline_refs(&buf);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].ref_name, "refs/heads/main");
        assert_eq!(refs[1].ref_name, "refs/heads/feature/test");
    }

    #[test]
    fn test_parse_pktline_with_capabilities() {
        let old = "1111111111111111111111111111111111111111";
        let new = "2222222222222222222222222222222222222222";
        // Premiere ligne contient les capabilities apres \0
        let content = format!(
            "{old} {new} refs/heads/main\0 report-status side-band-64k\n"
        );
        let mut buf = make_pktline(&content);
        buf.extend_from_slice(b"0000");

        let refs = parse_pktline_refs(&buf);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ref_name, "refs/heads/main");
        assert_eq!(refs[0].new_sha, new);
    }

    #[test]
    fn test_parse_pktline_empty_body() {
        let refs = parse_pktline_refs(b"");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_parse_pktline_flush_only() {
        let refs = parse_pktline_refs(b"0000");
        assert!(refs.is_empty());
    }

    #[test]
    fn test_pushed_ref_is_delete() {
        let r = PushedRef {
            old_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            new_sha: "0000000000000000000000000000000000000000".to_string(),
            ref_name: "refs/heads/old-branch".to_string(),
        };
        assert!(r.is_delete());
    }

    #[test]
    fn test_pushed_ref_is_not_delete() {
        let r = PushedRef {
            old_sha: "0000000000000000000000000000000000000000".to_string(),
            new_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ref_name: "refs/heads/new-branch".to_string(),
        };
        assert!(!r.is_delete());
    }

    #[test]
    fn test_pushed_ref_branch_name() {
        let r = PushedRef {
            old_sha: String::new(),
            new_sha: String::new(),
            ref_name: "refs/heads/feature/my-branch".to_string(),
        };
        assert_eq!(r.branch_name(), Some("feature/my-branch"));
    }

    #[test]
    fn test_pushed_ref_branch_name_tag() {
        let r = PushedRef {
            old_sha: String::new(),
            new_sha: String::new(),
            ref_name: "refs/tags/v1.0".to_string(),
        };
        assert_eq!(r.branch_name(), None); // tags ne sont pas des branches
    }
}
