//! Port: ContainerRunner — Phase 40 (Jutsu Runner Natif) 🥷⚡
//!
//! Contrat abstrait pour l'exécution de commandes dans un container OCI.
//! L'adaptateur concret dans infrastructure/ (JutsuRunner) implémente
//! le pilotage Docker via la crate `bollard`.
//!
//! ## Abstraction
//! Ce port isole le domaine/application de Docker/Podman.
//! En test, on peut injecter un mock qui simule des containers
//! sans daemon Docker réel.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;

use crate::errors::DomainError;

/// Résultat d'exécution d'un container OCI.
#[derive(Debug, Clone)]
pub struct ContainerRunResult {
    /// Code de sortie du processus dans le container.
    /// 0 = succès, tout autre valeur = échec.
    pub exit_code: i64,
    /// Logs stdout + stderr accumulés pendant l'exécution.
    pub logs: String,
    /// Durée d'exécution en millisecondes.
    pub duration_ms: i64,
}

/// Contrat pour l'exécution de stages dans des containers OCI.
///
/// Chaque appel à `run_stage()` crée un container éphémère,
/// exécute les commandes, collecte les logs, puis détruit le container.
///
/// ## Lifecycle d'un container
/// ```text
/// pull_image() → create_container() → start() → logs() → wait() → remove()
/// ```
///
/// ## Labels Docker (Vegapunk Micro-Tweak #2)
/// Chaque container créé porte les labels :
/// - `shinobi.runner=true` : identifie les containers Jutsu
/// - `shinobi.pipeline.id=<uuid>` : rattache au pipeline parent
/// Ces labels permettent un Garbage Collection des containers orphelins
/// en cas de crash de Taijutsu.
#[async_trait]
pub trait ContainerRunner: Send + Sync {
    /// Exécute un stage dans un container OCI.
    ///
    /// # Arguments
    /// - `image` : image Docker à pull/utiliser (ex: "rust:1.80-slim")
    /// - `commands` : liste de commandes bash à exécuter (enchaînées avec `&&`)
    /// - `workspace_path` : chemin du workspace éphémère monté en RW dans `/workspace`
    /// - `timeout` : durée maximale du container avant kill forcé
    /// - `pipeline_id` : UUID du pipeline parent (pour les labels Docker)
    ///
    /// # Errors
    /// - `DomainError::BusinessRule` : image introuvable
    /// - `DomainError::Internal` : Docker daemon inaccessible, timeout
    async fn run_stage(
        &self,
        image: &str,
        commands: &[String],
        workspace_path: &Path,
        timeout: Duration,
        pipeline_id: &str,
    ) -> Result<ContainerRunResult, DomainError>;
}
