//! Use Case: BulkImportGitHub — Import massif de dépôts GitHub (Phase 20B).
//!
//! ## Le Clonage Massif
//! Reçoit une liste d'URLs GitHub, les importe séquentiellement dans SHINOBI
//! via le `ImportGitHubRepoUseCase` existant. Skip les repos déjà importés.

use std::sync::Arc;

use serde::Serialize;
use tracing::{info, warn, instrument};
use uuid::Uuid;

use domain::errors::DomainError;
use domain::ports::repo_repository::RepoRepository;

use super::import_github_repo::{ImportGitHubRepoCommand, ImportGitHubRepoUseCase};

/// Commande pour l'import en masse.
#[derive(Debug, Clone)]
pub struct BulkImportCommand {
    /// ID de l'acteur SHINOBI qui importe les repos.
    pub owner_id: Uuid,
    /// Liste d'URLs GitHub à importer.
    pub repo_urls: Vec<String>,
}

/// Status d'un import individuel.
#[derive(Debug, Clone, Serialize)]
pub struct ImportItemStatus {
    /// URL du repo GitHub.
    pub github_url: String,
    /// Nom du repo dans SHINOBI (si importé avec succès).
    pub shinobi_name: Option<String>,
    /// Status : "imported", "skipped", "error".
    pub status: String,
    /// Message (raison du skip ou erreur).
    pub message: Option<String>,
}

/// Résultat de l'import en masse.
#[derive(Debug, Clone, Serialize)]
pub struct BulkImportResult {
    pub results: Vec<ImportItemStatus>,
    pub imported: u32,
    pub skipped: u32,
    pub failed: u32,
}

/// Use case : import massif de repos GitHub dans SHINOBI.
pub struct BulkImportGitHubUseCase {
    import_use_case: Arc<ImportGitHubRepoUseCase>,
    repo_repo: Arc<dyn RepoRepository>,
}

impl BulkImportGitHubUseCase {
    pub fn new(
        import_use_case: Arc<ImportGitHubRepoUseCase>,
        repo_repo: Arc<dyn RepoRepository>,
    ) -> Self {
        Self {
            import_use_case,
            repo_repo,
        }
    }

    #[instrument(skip(self), fields(owner_id = %cmd.owner_id, count = cmd.repo_urls.len()))]
    pub async fn execute(
        &self,
        cmd: BulkImportCommand,
    ) -> Result<BulkImportResult, DomainError> {
        let total = cmd.repo_urls.len();
        info!(total, "Phase 20B: Clonage Massif — démarrage");

        // Pré-charger les URLs déjà importées pour ce owner
        let existing_repos = self.repo_repo.list_by_owner(&cmd.owner_id).await?;
        let imported_urls: Vec<String> = existing_repos
            .iter()
            .filter_map(|r| r.mirror_source_url.clone())
            .map(|url| url.to_lowercase())
            .collect();

        let mut results = Vec::with_capacity(total);
        let mut imported = 0u32;
        let mut skipped = 0u32;
        let mut failed = 0u32;

        for (i, url) in cmd.repo_urls.iter().enumerate() {
            let progress = format!("[{}/{}]", i + 1, total);

            // Vérifier si déjà importé
            let normalized = url.trim().to_lowercase();
            let clone_url_guess = if normalized.ends_with(".git") {
                normalized.clone()
            } else {
                format!("{normalized}.git")
            };

            if imported_urls.iter().any(|u| u.contains(&normalized) || u == &clone_url_guess) {
                info!(
                    progress = %progress,
                    url = %url,
                    "⏭️ Skip — déjà importé"
                );
                results.push(ImportItemStatus {
                    github_url: url.clone(),
                    shinobi_name: None,
                    status: "skipped".to_string(),
                    message: Some("Déjà importé dans SHINOBI".to_string()),
                });
                skipped += 1;
                continue;
            }

            // Importer via le use case existant
            match self
                .import_use_case
                .execute(ImportGitHubRepoCommand {
                    owner_id: cmd.owner_id,
                    github_url: url.clone(),
                    name_override: None,
                })
                .await
            {
                Ok(repo) => {
                    info!(
                        progress = %progress,
                        repo_name = %repo.name,
                        "✅ Import réussi"
                    );
                    results.push(ImportItemStatus {
                        github_url: url.clone(),
                        shinobi_name: Some(repo.name),
                        status: "imported".to_string(),
                        message: None,
                    });
                    imported += 1;
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    // Traiter les duplicates comme des skips
                    if err_msg.contains("duplicate") || err_msg.contains("déjà") {
                        info!(
                            progress = %progress,
                            url = %url,
                            "⏭️ Skip — duplicate détecté pendant l'import"
                        );
                        results.push(ImportItemStatus {
                            github_url: url.clone(),
                            shinobi_name: None,
                            status: "skipped".to_string(),
                            message: Some("Déjà importé (duplicate détecté)".to_string()),
                        });
                        skipped += 1;
                    } else {
                        warn!(
                            progress = %progress,
                            url = %url,
                            error = %err_msg,
                            "❌ Erreur d'import"
                        );
                        results.push(ImportItemStatus {
                            github_url: url.clone(),
                            shinobi_name: None,
                            status: "error".to_string(),
                            message: Some(err_msg),
                        });
                        failed += 1;
                    }
                }
            }
        }

        info!(
            imported,
            skipped,
            failed,
            total,
            "Phase 20B: Clonage Massif terminé 🐙"
        );

        Ok(BulkImportResult {
            results,
            imported,
            skipped,
            failed,
        })
    }
}
