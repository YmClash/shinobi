//! Adaptateur Git CGI — Pont HTTP vers `git http-backend`.
//!
//! Utilise `tokio::process::Command` pour spawner le binaire CGI
//! `git-http-backend` et piper les flux HTTP (stdin/stdout).
//!
//! ## Architecture (Phase 12A — Le Raccourci Thermodynamique)
//! Plutot que de reimplementer le protocole Git Smart HTTP en Rust,
//! on delegue au binaire officiel `git http-backend` (CGI).
//! Axum ne fait que "piper" le flux HTTP directement dans le binaire.
//!
//! ## Protocole CGI
//! `git http-backend` est un programme CGI classique :
//! - Lit les variables d'environnement (PATH_INFO, QUERY_STRING, etc.)
//! - Lit le body depuis stdin
//! - Ecrit les headers CGI + body binaire sur stdout
//!
//! ## Localisation du binaire
//! - Windows : `{git --exec-path}/git-http-backend.exe`
//! - Linux   : `/usr/lib/git-core/git-http-backend`

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;

use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{info, instrument, warn};

use domain::errors::DomainError;

// ── Types publics ────────────────────────────────────────────────────

/// Reponse parsee du processus CGI.
#[derive(Debug)]
pub struct CgiResponse {
    /// Status HTTP extrait du header CGI `Status:` (defaut: 200).
    pub status: u16,
    /// Headers HTTP a retransmettre au client.
    pub headers: Vec<(String, String)>,
    /// Body binaire brut (packfile, pkt-line, etc.).
    pub body: Vec<u8>,
}

/// Backend CGI Git — wrapper autour de `git http-backend`.
///
/// Thread-safe (`Send + Sync`) — le `PathBuf` est immutable apres construction.
pub struct GitCgiBackend {
    /// Chemin absolu vers le binaire `git-http-backend`.
    http_backend_path: PathBuf,
}

impl GitCgiBackend {
    /// Construit un nouveau backend CGI en auto-detectant le binaire.
    ///
    /// Utilise `git --exec-path` pour trouver le repertoire contenant
    /// `git-http-backend`. Fonctionne sur Windows (Git for Windows)
    /// et Linux (package `git`).
    ///
    /// # Errors
    /// Retourne une erreur si `git` n'est pas dans le PATH ou si
    /// `git-http-backend` n'existe pas dans le exec-path.
    pub fn new() -> Result<Self, DomainError> {
        let exec_path = find_git_exec_path()?;

        let backend_name = if cfg!(windows) {
            "git-http-backend.exe"
        } else {
            "git-http-backend"
        };

        let http_backend_path = exec_path.join(backend_name);

        if !http_backend_path.exists() {
            return Err(DomainError::VcsError(format!(
                "git-http-backend not found at: {}",
                http_backend_path.display()
            )));
        }

        info!(
            path = %http_backend_path.display(),
            "GitCgiBackend initialise (git http-backend detecte)"
        );

        Ok(Self { http_backend_path })
    }

    /// Execute le CGI `git http-backend` avec les parametres HTTP donnes.
    ///
    /// # Arguments
    /// - `repo_git_path` : Chemin vers le bare Git repo (`.jj/repo/store/git`)
    /// - `method` : `GET` ou `POST`
    /// - `path_info` : Chemin relatif (ex: `/info/refs`, `/git-receive-pack`)
    /// - `query_string` : Query string (ex: `service=git-receive-pack`)
    /// - `content_type` : Content-Type du body (pour POST)
    /// - `body` : Body de la requete (packfile pour receive-pack)
    ///
    /// # Returns
    /// `CgiResponse` avec le status, headers et body parses depuis stdout.
    #[instrument(skip(self, body), fields(method = %method, path_info = %path_info))]
    pub async fn execute_cgi(
        &self,
        repo_git_path: &Path,
        method: &str,
        path_info: &str,
        query_string: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<CgiResponse, DomainError> {
        // Construire la commande CGI avec les variables d'environnement
        let mut cmd = Command::new(&self.http_backend_path);

        // git http-backend attend :
        //   GIT_PROJECT_ROOT = repertoire parent contenant les repos
        //   PATH_INFO = /{nom_repo}/action (ex: /git/info/refs)
        //
        // On decompose le chemin du bare repo :
        //   repo_git_path = /shinobi_vcs/uuid/.jj/repo/store/git
        //   → project_root = /shinobi_vcs/uuid/.jj/repo/store
        //   → repo_name = "git"
        //   → full_path_info = /git/info/refs
        let project_root = repo_git_path.parent().unwrap_or(repo_git_path);
        let repo_dir_name = repo_git_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("git");
        let full_path_info = format!("/{repo_dir_name}{path_info}");

        // Variables CGI requises par git http-backend
        cmd.env("GIT_PROJECT_ROOT", project_root)
            .env("GIT_HTTP_EXPORT_ALL", "1")
            .env("PATH_INFO", &full_path_info)
            .env("QUERY_STRING", query_string)
            .env("REQUEST_METHOD", method)
            .env("CONTENT_TYPE", content_type);

        // ── Phase 12A Fix : Autoriser push + clone via GIT_CONFIG_* ──
        // Par defaut, git http-backend refuse les pushes (receive-pack)
        // sauf si `http.receivepack = true` est configure.
        // On injecte la config via les env vars Git 2.31+ pour ne pas
        // modifier le repo sur disque.
        cmd.env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "http.receivepack")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env("GIT_CONFIG_KEY_1", "http.uploadpack")
            .env("GIT_CONFIG_VALUE_1", "true");

        // Pour POST, indiquer la taille du body
        if method == "POST" && !body.is_empty() {
            cmd.env("CONTENT_LENGTH", body.len().to_string());
        }

        // Piper stdin/stdout/stderr
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Spawner le processus
        let mut child = cmd.spawn().map_err(|e| {
            DomainError::VcsError(format!(
                "Failed to spawn git-http-backend: {e}"
            ))
        })?;

        // Ecrire le body dans stdin (pour POST)
        if !body.is_empty() {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(body).await.map_err(|e| {
                    DomainError::VcsError(format!("Failed to write to git stdin: {e}"))
                })?;
                // Drop stdin pour signaler EOF au processus
                drop(stdin);
            }
        } else {
            // Fermer stdin immediatement pour les requetes GET
            drop(child.stdin.take());
        }

        // Attendre la fin du processus et lire stdout/stderr
        let output = child.wait_with_output().await.map_err(|e| {
            DomainError::VcsError(format!("git-http-backend wait failed: {e}"))
        })?;

        // Verifier le code de sortie du processus
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "git-http-backend returned non-zero exit code"
            );
            return Err(DomainError::VcsError(format!(
                "git-http-backend failed (exit {:?}): {}",
                output.status.code(),
                stderr.trim()
            )));
        }

        // Parser la sortie CGI (headers + body)
        let response = parse_cgi_output(&output.stdout);

        info!(
            status = response.status,
            headers_count = response.headers.len(),
            body_size = response.body.len(),
            "git-http-backend CGI response parsed"
        );

        Ok(response)
    }

    /// Execute le CGI `git http-backend` en **streaming** — N'ALLOUE PAS
    /// le body complet en RAM.
    ///
    /// ## Phase 36 — DEBT-007 (Streaming CGI)
    ///
    /// Contrairement a `execute_cgi()` qui prend un `&[u8]` (body entier en RAM),
    /// cette methode recoit un `prefix` (les quelques centaines d'octets de
    /// pkt-line deja consommes par le parser de refs) et un `remaining_stream`
    /// (le flux binaire du PACK file potentiellement volumineux).
    ///
    /// Le flux est pipe directement dans le stdin du child process chunk par
    /// chunk, sans jamais stocker l'integralite du PACK en memoire.
    ///
    /// ## Garanties
    /// - Utilise `tokio::process::ChildStdin` (async) — ne bloque PAS le runtime
    /// - Pic memoire ~ taille d'un chunk (~64 Ko) au lieu de la taille totale du push
    /// - Le `content_length` est relaye depuis le header HTTP client (pas calcule)
    ///
    /// ## Protocole
    /// `git http-backend` lit stdin sequentiellement : d'abord les pkt-line refs,
    /// puis le PACK binaire. Le prefix est ecrit en premier, suivi du flux restant.
    /// Le drop de stdin signale EOF → le CGI commence le traitement.
    #[instrument(skip(self, prefix, remaining_stream), fields(path_info = %path_info))]
    pub async fn execute_cgi_streaming(
        &self,
        repo_git_path: &Path,
        path_info: &str,
        content_type: &str,
        content_length: Option<String>,
        prefix: Vec<u8>,
        mut remaining_stream: Pin<Box<dyn futures::Stream<Item = Result<Vec<u8>, String>> + Send>>,
    ) -> Result<CgiResponse, DomainError> {
        let project_root = repo_git_path.parent().unwrap_or(repo_git_path);
        let repo_dir_name = repo_git_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("git");
        let full_path_info = format!("/{repo_dir_name}{path_info}");

        let mut cmd = Command::new(&self.http_backend_path);

        cmd.env("GIT_PROJECT_ROOT", project_root)
            .env("GIT_HTTP_EXPORT_ALL", "1")
            .env("PATH_INFO", &full_path_info)
            .env("QUERY_STRING", "")
            .env("REQUEST_METHOD", "POST")
            .env("CONTENT_TYPE", content_type);

        cmd.env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "http.receivepack")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env("GIT_CONFIG_KEY_1", "http.uploadpack")
            .env("GIT_CONFIG_VALUE_1", "true");

        // Relayer CONTENT_LENGTH depuis le header HTTP client.
        // Si absent (chunked transfer), ne pas le set — git http-backend
        // sait lire jusqu'a EOF.
        if let Some(cl) = &content_length {
            cmd.env("CONTENT_LENGTH", cl);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            DomainError::VcsError(format!("Failed to spawn git-http-backend (streaming): {e}"))
        })?;

        // ── Piping stdin : prefix + stream (async, zero copie globale) ──
        if let Some(mut stdin) = child.stdin.take() {
            // 1. Ecrire le prefix (pkt-line refs deja consommes, ~200 bytes)
            if !prefix.is_empty() {
                stdin.write_all(&prefix).await.map_err(|e| {
                    DomainError::VcsError(format!("Failed to write prefix to git stdin: {e}"))
                })?;
            }

            // 2. Pipe le flux restant chunk par chunk
            //    Utilise tokio::process::ChildStdin (async) — NE BLOQUE PAS le runtime
            let mut bytes_piped: u64 = prefix.len() as u64;
            while let Some(chunk_result) = remaining_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        stdin.write_all(&chunk).await.map_err(|e| {
                            DomainError::VcsError(format!(
                                "Failed to pipe stream to git stdin: {e}"
                            ))
                        })?;
                        bytes_piped += chunk.len() as u64;
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            bytes_piped = bytes_piped,
                            "CGI streaming: erreur lecture body stream — fermeture stdin"
                        );
                        break; // Drop stdin → EOF prématuré → le CGI gère l'erreur
                    }
                }
            }

            // 3. Drop stdin → EOF → le CGI commence le traitement du pack
            drop(stdin);

            info!(
                bytes_piped = bytes_piped,
                "CGI streaming: stdin pipe complete"
            );
        }

        // Attendre la fin du processus et lire stdout/stderr
        let output = child.wait_with_output().await.map_err(|e| {
            DomainError::VcsError(format!("git-http-backend (streaming) wait failed: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                exit_code = ?output.status.code(),
                stderr = %stderr,
                "git-http-backend (streaming) returned non-zero exit code"
            );
            return Err(DomainError::VcsError(format!(
                "git-http-backend failed (exit {:?}): {}",
                output.status.code(),
                stderr.trim()
            )));
        }

        let response = parse_cgi_output(&output.stdout);

        info!(
            status = response.status,
            headers_count = response.headers.len(),
            body_size = response.body.len(),
            "git-http-backend CGI streaming response parsed"
        );

        Ok(response)
    }
}

// ── Fonctions utilitaires ────────────────────────────────────────────

/// Trouve le chemin du repertoire `exec-path` de Git.
///
/// Execute `git --exec-path` et parse la sortie.
fn find_git_exec_path() -> Result<PathBuf, DomainError> {
    let output = std::process::Command::new("git")
        .arg("--exec-path")
        .output()
        .map_err(|e| {
            DomainError::VcsError(format!(
                "Failed to run 'git --exec-path' — is git installed? {e}"
            ))
        })?;

    if !output.status.success() {
        return Err(DomainError::VcsError(
            "git --exec-path returned non-zero exit code".to_string(),
        ));
    }

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if path_str.is_empty() {
        return Err(DomainError::VcsError(
            "git --exec-path returned empty string".to_string(),
        ));
    }

    Ok(PathBuf::from(path_str))
}

/// Parse la sortie brute d'un processus CGI en headers + body.
///
/// La sortie CGI est : `Header1: value1\r\nHeader2: value2\r\n\r\nbody_bytes`
/// On cherche la sequence `\r\n\r\n` (octets `[13, 10, 13, 10]`) pour separer.
///
/// Si la sequence n'est pas trouvee, tout est considere comme body
/// (fallback pour les erreurs CGI).
pub fn parse_cgi_output(raw: &[u8]) -> CgiResponse {
    // Chercher la frontiere headers/body : \r\n\r\n
    let separator = b"\r\n\r\n";
    let split_pos = raw
        .windows(separator.len())
        .position(|w| w == separator);

    let (header_bytes, body) = match split_pos {
        Some(pos) => (&raw[..pos], raw[pos + separator.len()..].to_vec()),
        None => {
            // Pas de separateur — essayer avec \n\n (certains CGI sur Linux)
            let alt_separator = b"\n\n";
            let alt_pos = raw
                .windows(alt_separator.len())
                .position(|w| w == alt_separator);

            match alt_pos {
                Some(pos) => (&raw[..pos], raw[pos + alt_separator.len()..].to_vec()),
                None => {
                    // Aucun separateur — tout est body (fallback)
                    return CgiResponse {
                        status: 200,
                        headers: vec![],
                        body: raw.to_vec(),
                    };
                }
            }
        }
    };

    // Parser les headers
    let header_str = String::from_utf8_lossy(header_bytes);
    let mut headers = Vec::new();
    let mut status: u16 = 200;

    for line in header_str.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            // Extraire le status du header CGI "Status: 200 OK"
            if key.eq_ignore_ascii_case("Status") {
                if let Some(code_str) = value.split_whitespace().next() {
                    if let Ok(code) = code_str.parse::<u16>() {
                        status = code;
                    }
                }
            }

            headers.push((key, value));
        }
    }

    CgiResponse {
        status,
        headers,
        body,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_exec_path_found() {
        let result = find_git_exec_path();
        assert!(
            result.is_ok(),
            "git --exec-path should succeed: {:?}",
            result.err()
        );
        let path = result.unwrap();
        assert!(
            path.exists(),
            "git exec-path should exist: {}",
            path.display()
        );
    }

    #[test]
    fn test_git_http_backend_binary_exists() {
        let exec_path = find_git_exec_path().unwrap();
        let backend_name = if cfg!(windows) {
            "git-http-backend.exe"
        } else {
            "git-http-backend"
        };
        let backend_path = exec_path.join(backend_name);
        assert!(
            backend_path.exists(),
            "git-http-backend should exist at: {}",
            backend_path.display()
        );
    }

    #[test]
    fn test_parse_cgi_output_splits_headers_body() {
        let raw = b"Content-Type: application/x-git-upload-pack-advertisement\r\nCache-Control: no-cache\r\n\r\n001e# service=git-upload-pack\n";
        let response = parse_cgi_output(raw);

        assert_eq!(response.status, 200);
        assert_eq!(response.headers.len(), 2);
        assert_eq!(response.headers[0].0, "Content-Type");
        assert_eq!(
            response.headers[0].1,
            "application/x-git-upload-pack-advertisement"
        );
        assert_eq!(response.headers[1].0, "Cache-Control");
        assert_eq!(response.headers[1].1, "no-cache");
        assert!(!response.body.is_empty());
    }

    #[test]
    fn test_parse_cgi_output_extracts_status() {
        let raw = b"Status: 404 Not Found\r\nContent-Type: text/plain\r\n\r\nRepository not found";
        let response = parse_cgi_output(raw);

        assert_eq!(response.status, 404);
        assert_eq!(response.headers.len(), 2);
        assert_eq!(
            String::from_utf8_lossy(&response.body),
            "Repository not found"
        );
    }

    #[test]
    fn test_parse_cgi_output_no_separator_fallback() {
        let raw = b"raw binary data without any headers";
        let response = parse_cgi_output(raw);

        assert_eq!(response.status, 200);
        assert!(response.headers.is_empty());
        assert_eq!(response.body, raw);
    }

    #[tokio::test]
    async fn test_cgi_backend_new_succeeds() {
        let result = GitCgiBackend::new();
        assert!(
            result.is_ok(),
            "GitCgiBackend::new() should succeed if git is installed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_cgi_info_refs_on_bare_repo() {
        // Creer un bare repo Git temporaire
        let tmp = tempfile::TempDir::new().expect("Failed to create temp dir");
        let bare_path = tmp.path().join("test.git");

        let init_output = std::process::Command::new("git")
            .args(["init", "--bare"])
            .arg(&bare_path)
            .output()
            .expect("git init --bare failed");
        assert!(init_output.status.success(), "git init --bare should succeed");

        // Executer le CGI info/refs
        let backend = GitCgiBackend::new().expect("GitCgiBackend::new failed");
        let response = backend
            .execute_cgi(
                &bare_path,
                "GET",
                "/info/refs",
                "service=git-upload-pack",
                "",
                &[],
            )
            .await;

        assert!(
            response.is_ok(),
            "CGI info/refs should succeed: {:?}",
            response.err()
        );

        let resp = response.unwrap();
        assert_eq!(resp.status, 200, "CGI should return 200");

        // Verifier le Content-Type Git
        let content_type = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(
            content_type,
            Some("application/x-git-upload-pack-advertisement"),
            "Content-Type should be git upload-pack advertisement"
        );
    }
}
