//! Port: GitHubService — Contrat d'interaction avec l'API GitHub.
//!
//! Ce trait abstrait les appels à l'API GitHub (métadonnées de repos)
//! et les opérations de clone Git (fetch mirror).
//!
//! ## Phase 19B — Le Pont des Mondes
//! Permet l'import de dépôts GitHub publics dans la Forge SHINOBI.
//! V1 : uniquement les repos publics (pas de PAT GitHub).

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Métadonnées d'un dépôt GitHub récupérées via l'API publique v3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepoInfo {
    /// Nom complet (ex: "tokio-rs/tokio").
    pub full_name: String,
    /// Nom court (ex: "tokio").
    pub name: String,
    /// Description du dépôt (optionnel).
    pub description: Option<String>,
    /// URL de clone HTTPS (ex: "https://github.com/tokio-rs/tokio.git").
    pub clone_url: String,
    /// Branche par défaut (ex: "main").
    pub default_branch: String,
    /// Nombre de stars.
    pub stars: u32,
    /// Nombre de forks.
    pub forks: u32,
    /// Langage principal (ex: "Rust").
    pub language: Option<String>,
    /// Licence SPDX (ex: "MIT").
    pub license: Option<String>,
    /// `true` si le dépôt est privé.
    pub is_private: bool,
}

/// Port d'interaction avec GitHub pour l'import de dépôts.
#[async_trait]
pub trait GitHubService: Send + Sync {
    /// Récupère les métadonnées d'un repo GitHub via l'API publique v3.
    ///
    /// ## Erreurs
    /// - `NotFound` si le repo n'existe pas ou est privé.
    /// - `External` si l'API GitHub est indisponible ou rate-limitée (403).
    async fn fetch_repo_info(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<GitHubRepoInfo, DomainError>;

    /// Injecte le contenu d'un repo GitHub dans un bare Git repo existant.
    ///
    /// ## Stratégie (Le Fetch Injecté)
    /// 1. Ajoute `origin` comme remote dans le bare repo
    /// 2. `git fetch origin +refs/*:refs/*` — aspire toutes les refs
    ///
    /// Cette méthode préserve la structure interne du workspace Jujutsu
    /// (HEAD, config) contrairement à un `clone --mirror` destructif.
    async fn fetch_into_bare(
        &self,
        clone_url: &str,
        bare_repo_path: &Path,
    ) -> Result<(), DomainError>;

    /// Liste les dépôts GitHub de l'utilisateur authentifié (Phase 20B).
    ///
    /// Utilise le token OAuth pour appeler `GET /user/repos`.
    /// Limité aux dépôts publics en V1 (scope `public_repo`).
    ///
    /// ## Arguments
    /// - `access_token` : token OAuth GitHub de l'utilisateur
    /// - `per_page` : nombre de repos par page (max 100)
    async fn list_user_repos(
        &self,
        access_token: &str,
        per_page: u32,
    ) -> Result<Vec<GitHubRepoInfo>, DomainError>;
}
