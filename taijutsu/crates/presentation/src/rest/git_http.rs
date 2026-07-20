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
//! ## Authentification (Phase 19A-Git — Les Portes de Fer)
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

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Router, routing::get, routing::post};
use base64::Engine;
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
    let encoded = header.strip_prefix("Basic ")
        .or_else(|| header.strip_prefix("basic "))?;
    let decoded = String::from_utf8(
        base64::engine::general_purpose::STANDARD.decode(encoded).ok()?
    ).ok()?;
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
async fn authenticate_pat(
    state: &GitHttpState,
    headers: &HeaderMap,
) -> Result<Actor, Response> {
    let (username, password) = extract_basic_auth(headers)
        .ok_or_else(|| challenge_401("Authentication required — use a Personal Access Token"))?;

    // Hash SHA-256 du PAT brut pour lookup
    let pat_hash = state.auth_service.hash_pat_for_lookup(&password);

    // Reverse lookup : hash → actor
    let actor = state.actor_repo
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
    // Sinon, verifier si collaborateur
    state.repo_repo
        .is_collaborator(&actor.id, &repository.id)
        .await
        .unwrap_or(false)
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
///
/// ## Auth (Phase 19A-Git)
/// - Si `service=git-receive-pack` (push) : PAT obligatoire
/// - Si `service=git-upload-pack` (clone) : Public (V1)
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

    // 1b. Auth conditionnelle : exiger PAT si push (Phase 19A-Git)
    if query.service.as_deref() == Some("git-receive-pack") {
        let actor = match authenticate_pat(&state, &headers).await {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        // Verifier l'ownership/collaboration
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

    // 2. Construire le chemin vers le bare Git repo
    let repo_git_path = state.vcs_engine.git_repo_path(&repository.id);

    // Lazy init : si le bare Git repo n'existe pas (repo cree avec
    // SimpleBackend avant Phase 11, ou workspace non initialise),
    // on initialise le workspace jj+GitBackend a la volee.
    if !repo_git_path.exists() {
        info!(
            repo_id = %repository.id,
            path = %repo_git_path.display(),
            "Git HTTP: bare Git repo absent — lazy init du workspace"
        );
        if let Err(e) = state.vcs_engine.init_workspace(&repository.owner_id, &repository.id).await {
            warn!(error = %e, "Git HTTP: lazy init failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to initialize Git repository").into_response();
        }
        // Re-verifier apres init
        if !repo_git_path.exists() {
            warn!(
                path = %repo_git_path.display(),
                "Git HTTP: bare Git repo toujours absent apres init"
            );
            return (StatusCode::NOT_FOUND, "Git repository not initialized").into_response();
        }
        info!(
            repo_id = %repository.id,
            "Git HTTP: workspace initialise avec succes (lazy init)"
        );
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
///
/// ## Auth (Phase 19A-Git)
/// Public en V1 — pas de repos prives. L'auth sera ajoutee quand
/// le concept `visibility = 'private'` sera implemente (Phase 20).
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
/// ## Flux (Phase 12A-Fix — enrichi)
/// 1. `reload_repo()` — jj voit les nouveaux commits Git
/// 2. `resolve_head()` — recupere le nouveau SHA-1
/// 3. `read_commit_snapshot()` — lit les fichiers + description du commit
/// 4. `store_dag()` — stocke les fichiers sur IPFS (graceful degradation)
/// 5. Construit une `Operation` complete (description + IPFS CID)
/// 6. Persiste dans PostgreSQL
/// 7. Publie l'etincelle Kafka (fire-and-forget)
async fn sync_hook_post_push(
    state: &GitHttpState,
    repository: &domain::entities::repository::Repository,
) -> Result<(), domain::errors::DomainError> {
    let repo_id = repository.id;

    // 0. S'assurer que le workspace est enregistre dans le DashMap
    state.vcs_engine.init_workspace(&repository.owner_id, &repo_id).await?;

    // 1. Recharger le repo jj (pour voir les nouveaux commits Git)
    state.vcs_engine.reload_repo(&repo_id).await?;

    // 2. Resoudre le nouveau HEAD depuis les refs Git du bare repo.
    //    On utilise resolve_git_head() au lieu de resolve_head() car :
    //    - resolve_head() trie les heads JJ et peut retourner un ancien
    //      commit orphelin cree par un precedent Sync Hook
    //    - resolve_git_head() lit directement refs/heads/main dans le
    //      bare git repo, garanti d'etre le commit qui vient d'etre pushe
    let content_id = state
        .vcs_engine
        .resolve_git_head(&repo_id)
        .ok_or_else(|| {
            domain::errors::DomainError::VcsError(
                "No git HEAD after reload — empty repo?".to_string(),
            )
        })?;

    info!(
        repo_id = %repo_id,
        head = %content_id,
        "Sync Hook: HEAD Git resolu apres push (refs/heads)"
    );

    // 3. Lire le snapshot du commit (fichiers + description)
    let snapshot = state
        .vcs_engine
        .read_commit_snapshot(&repo_id, &content_id)
        .await?;

    let description = if snapshot.description.trim().is_empty() {
        "External git push".to_string()
    } else {
        snapshot.description.trim().to_string()
    };

    info!(
        repo_id = %repo_id,
        description = %description.chars().take(80).collect::<String>(),
        file_count = snapshot.files.len(),
        "Sync Hook: snapshot lu (fichiers + description)"
    );

    // 4. Stocker les fichiers sur IPFS via store_dag (graceful degradation)
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

    // 5. Résoudre les parents Git → UUID Operation (La Lignée Sanguine — Phase 12A-Fix2)
    let mut parent_op_ids: Vec<uuid::Uuid> = Vec::new();
    for parent_sha in &snapshot.parent_commit_ids {
        match state.operation_repo.find_by_content_id(&repo_id, parent_sha).await {
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

    // 6. Construire l'entite Operation complete
    let operation = Operation::new(
        repository.owner_id,
        repository.id,
        content_id,
        ipfs_cid,
        &description,
        parent_op_ids,
    );

    // 6. Persister dans PostgreSQL (avec detection de doublon)
    //    Si le meme content_id existe deja (push idempotent ou re-push),
    //    on skip silencieusement au lieu de crasher.
    match state.operation_repo.save(&operation).await {
        Ok(()) => {
            info!(
                operation_id = %operation.id,
                repo_id = %repo_id,
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

    // 7. Publier l'etincelle Kafka (fire-and-forget)
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
        "Sync Hook: Phase 12A complete (PG + IPFS + Kafka)"
    );

    Ok(())
}
