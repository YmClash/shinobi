//! Use Case: AnalyzeOperation — Analyse sémantique d'une opération VCS.
//!
//! C'est ici que l'IA s'éveille. Ce use case est le cœur de l'agent Tensai :
//! il reçoit une `Operation` publiée sur Kafka, télécharge les fichiers
//! depuis IPFS (Genjutsu), et utilise Tree-sitter pour découper le code
//! en fragments sémantiques (fonctions, structs, traits, impls).
//!
//! ## Flux
//! 1. Vérifie que l'opération contient un CID IPFS (`ipfs_content_id`)
//! 2. Télécharge le blob JSON depuis IPFS via `ContentStore`
//! 3. Décode les fichiers (chemin + contenu base64)
//! 4. Filtre les fichiers par langage supporté (`.rs` pour l'instant)
//! 5. Découpe chaque fichier via le `Chunker` (Tree-sitter)
//! 6. Produit un `AnalysisReport` avec tous les chunks extraits
//!
//! ## Phase 6B
//! - Persistence des chunks dans PostgreSQL via `ChunkRepository` (UNNEST batch).
//! - Le `EventPublisher` est préparé (slot) mais non branché.
//! - Phase 7 : re-publication sur `shinobi.tensai.analysis-complete`.

use std::sync::Arc;
use std::time::Instant;

use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::chunk_repository::{ChunkRepository, StoredChunk};
use domain::ports::content_store::ContentStore;

use tensai::{Chunker, SemanticChunk};

/// Rapport d'analyse sémantique d'une opération VCS.
///
/// Contient tous les fragments de code extraits par Tree-sitter,
/// ainsi que les métriques de l'analyse.
#[derive(Debug)]
pub struct AnalysisReport {
    /// ID de l'opération analysée.
    pub operation_id: Uuid,
    /// CID IPFS du blob analysé.
    pub ipfs_cid: String,
    /// Nombre de fichiers analysés (filtrés par langage supporté).
    pub analyzed_files: usize,
    /// Nombre de fichiers ignorés (langage non supporté).
    pub skipped_files: usize,
    /// Nombre total de chunks sémantiques extraits.
    pub total_chunks: usize,
    /// Fragments sémantiques extraits.
    pub chunks: Vec<SemanticChunk>,
    /// Durée de l'analyse en millisecondes.
    pub duration_ms: u64,
}

/// Résultat de l'analyse : soit un rapport complet, soit un skip motivé.
#[derive(Debug)]
pub enum AnalysisOutcome {
    /// Analyse complète avec rapport.
    Analyzed(AnalysisReport),
    /// Opération ignorée (pas de CID IPFS ou pas de fichiers).
    Skipped { operation_id: Uuid, reason: String },
}

/// Use case: analyser sémantiquement les fichiers d'une opération VCS.
///
/// Reçoit les ports en injection (Arc<dyn Trait>),
/// garantissant l'inversion de dépendance.
///
/// ## Persistence (Phase 6B)
/// Les chunks sont persistés dans PostgreSQL via `ChunkRepository`
/// (UNNEST batch insert). Si le repo est `None`, les chunks sont
/// seulement loggés (graceful degradation).
///
/// ## Slot EventPublisher (Phase 7)
/// Le champ `event_publisher` est réservé pour la re-publication
/// d'un événement `shinobi.tensai.analysis-complete` après analyse.
pub struct AnalyzeOperationUseCase {
    content_store: Arc<dyn ContentStore>,
    chunker: Arc<dyn Chunker>,
    /// Phase 6B — Persistence des chunks dans PostgreSQL.
    chunk_repository: Option<Arc<dyn ChunkRepository>>,
    // Phase 7 — Re-publication événementielle.
    // Slot préparé mais non branché : l'agent Tensai pourra publier
    // un événement `analysis-complete` pour les agents en aval.
    #[allow(dead_code)]
    event_publisher: Option<Arc<dyn domain::ports::event_publisher::EventPublisher>>,
}

impl AnalyzeOperationUseCase {
    /// Construit le use case avec ses dépendances injectées.
    ///
    /// # Arguments
    /// - `content_store` : accès au stockage distribué IPFS (Genjutsu)
    /// - `chunker` : moteur de découpage sémantique (Tree-sitter)
    /// - `chunk_repository` : persistence des chunks (Phase 6B, `None` = log only)
    /// - `event_publisher` : slot pour re-publication (Phase 7, `None` pour l'instant)
    pub fn new(
        content_store: Arc<dyn ContentStore>,
        chunker: Arc<dyn Chunker>,
        chunk_repository: Option<Arc<dyn ChunkRepository>>,
        event_publisher: Option<Arc<dyn domain::ports::event_publisher::EventPublisher>>,
    ) -> Self {
        Self {
            content_store,
            chunker,
            chunk_repository,
            event_publisher,
        }
    }

    /// Exécute l'analyse sémantique d'une opération.
    ///
    /// ## Flux
    /// 1. Vérifie `ipfs_content_id` → skip si absent
    /// 2. Télécharge le blob JSON depuis IPFS
    /// 3. Décode fichiers (path + content_b64)
    /// 4. Filtre `.rs` → chunk via Tree-sitter
    /// 5. Retourne `AnalysisReport`
    pub async fn execute(
        &self,
        operation: &Operation,
    ) -> Result<AnalysisOutcome, DomainError> {
        let start = Instant::now();

        // 1. Vérifier la présence du CID IPFS.
        let ipfs_cid = match &operation.ipfs_content_id {
            Some(cid) => cid.clone(),
            None => {
                info!(
                    operation_id = %operation.id,
                    "⏭️ Tensai — Skip: pas de CID IPFS (opération sans fichiers)"
                );
                return Ok(AnalysisOutcome::Skipped {
                    operation_id: operation.id,
                    reason: "Pas de CID IPFS".to_string(),
                });
            }
        };

        // 2. Télécharger le blob JSON depuis IPFS (Genjutsu).
        let blob = self.content_store.retrieve(&ipfs_cid).await?;

        info!(
            operation_id = %operation.id,
            ipfs_cid = %ipfs_cid,
            blob_size = blob.len(),
            "📥 Tensai — Blob IPFS récupéré"
        );

        // 3. Décoder le blob JSON → liste de fichiers.
        let files: Vec<FileEntry> = serde_json::from_slice(&blob).map_err(|e| {
            DomainError::Internal(format!(
                "Tensai — Décodage JSON du blob IPFS échoué: {e}"
            ))
        })?;

        if files.is_empty() {
            return Ok(AnalysisOutcome::Skipped {
                operation_id: operation.id,
                reason: "Blob IPFS vide (aucun fichier)".to_string(),
            });
        }

        // 4. Filtrer et analyser les fichiers par langage.
        let mut all_chunks: Vec<SemanticChunk> = Vec::new();
        let mut analyzed_files = 0usize;
        let mut skipped_files = 0usize;

        for file in &files {
            // Détecter le langage à partir de l'extension.
            let language = match detect_language(&file.path) {
                Some(lang) => lang,
                None => {
                    skipped_files += 1;
                    continue;
                }
            };

            // Vérifier que le chunker supporte ce langage.
            if !self.chunker.supported_languages().contains(&language.to_string()) {
                skipped_files += 1;
                continue;
            }

            // Décoder le contenu base64 → source UTF-8.
            let decoded = match base64_decode(&file.content_b64) {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!(
                        path = %file.path,
                        error = %e,
                        "⚠️ Tensai — Décodage base64 échoué, skip fichier"
                    );
                    skipped_files += 1;
                    continue;
                }
            };

            let source = match String::from_utf8(decoded) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        path = %file.path,
                        error = %e,
                        "⚠️ Tensai — Contenu non-UTF8, skip fichier"
                    );
                    skipped_files += 1;
                    continue;
                }
            };

            // 5. Découper le fichier via Tree-sitter (Tensai).
            match self.chunker.chunk(&source, &file.path, language).await {
                Ok(chunks) => {
                    info!(
                        path = %file.path,
                        chunk_count = chunks.len(),
                        "🔍 Tensai — Fichier analysé"
                    );
                    all_chunks.extend(chunks);
                    analyzed_files += 1;
                }
                Err(e) => {
                    warn!(
                        path = %file.path,
                        error = %e,
                        "⚠️ Tensai — Erreur de parsing Tree-sitter"
                    );
                    skipped_files += 1;
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // 6. Construire le rapport.
        let report = AnalysisReport {
            operation_id: operation.id,
            ipfs_cid: ipfs_cid.to_string(),
            analyzed_files,
            skipped_files,
            total_chunks: all_chunks.len(),
            chunks: all_chunks,
            duration_ms,
        };

        // Log structuré.
        log_analysis_report(&report);

        // Phase 6B — Persistence des chunks dans PostgreSQL.
        if let Some(repo) = &self.chunk_repository {
            // Conversion SemanticChunk → StoredChunk (frontière application/domain).
            let stored: Vec<StoredChunk> = report
                .chunks
                .iter()
                .map(|c| StoredChunk {
                    kind: format!("{:?}", c.kind).to_lowercase(),
                    name: c.name.clone(),
                    content: c.content.clone(),
                    start_line: c.start_line,
                    end_line: c.end_line,
                    file_path: c.file_path.clone(),
                    language: c.language.clone(),
                })
                .collect();

            match repo.save_chunks(&report.operation_id, &stored).await {
                Ok(saved) => {
                    info!(
                        operation_id = %report.operation_id,
                        saved_chunks = saved,
                        "💾 Tensai — Chunks persistés dans PostgreSQL"
                    );
                }
                Err(e) => {
                    warn!(
                        operation_id = %report.operation_id,
                        error = %e,
                        "⚠️ Tensai — Erreur de persistence des chunks (non-fatal)"
                    );
                }
            }
        }

        // Phase 7 — Re-publication événementielle (slot préparé).
        // if let Some(publisher) = &self.event_publisher {
        //     publisher.publish_analysis_complete(&report).await?;
        // }

        Ok(AnalysisOutcome::Analyzed(report))
    }
}

// ── Types internes ────────────────────────────────────────────────────

/// Entrée de fichier dans le blob JSON IPFS.
///
/// Correspond au format produit par `CreateOperationUseCase::sync_to_ipfs()`.
#[derive(Debug, serde::Deserialize)]
struct FileEntry {
    /// Chemin du fichier (ex: "src/main.rs").
    path: String,
    /// Contenu encodé en base64 (RFC 4648).
    content_b64: String,
    /// Taille originale en bytes.
    #[allow(dead_code)]
    size: usize,
}

/// Détecte le langage de programmation à partir de l'extension du fichier.
///
/// Retourne `None` si le fichier n'est pas un fichier source reconnu.
fn detect_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust"),
        // Phase 7 : ajouter "ts" => "typescript", "py" => "python", etc.
        _ => None,
    }
}

/// Log structuré du rapport d'analyse — Phase 6A (pas de persistence).
fn log_analysis_report(report: &AnalysisReport) {
    // Compter les types de chunks pour un log informatif.
    let mut functions = 0usize;
    let mut structs = 0usize;
    let mut traits = 0usize;
    let mut impls = 0usize;
    let mut enums = 0usize;
    let mut imports = 0usize;
    let mut others = 0usize;

    for chunk in &report.chunks {
        match chunk.kind {
            tensai::ChunkKind::Function => functions += 1,
            tensai::ChunkKind::Struct => structs += 1,
            tensai::ChunkKind::Trait => traits += 1,
            tensai::ChunkKind::Impl => impls += 1,
            tensai::ChunkKind::Enum => enums += 1,
            tensai::ChunkKind::Import => imports += 1,
            _ => others += 1,
        }
    }

    info!(
        operation_id = %report.operation_id,
        ipfs_cid = %report.ipfs_cid,
        analyzed_files = report.analyzed_files,
        skipped_files = report.skipped_files,
        total_chunks = report.total_chunks,
        functions,
        structs,
        traits,
        impls,
        enums,
        imports,
        others,
        duration_ms = report.duration_ms,
        "🧠 Tensai — Analyse sémantique terminée"
    );

    // Log détaillé de chaque chunk pour le debug.
    for chunk in &report.chunks {
        info!(
            kind = ?chunk.kind,
            name = ?chunk.name,
            file = %chunk.file_path,
            lines = format!("{}:{}", chunk.start_line, chunk.end_line),
            "   📦 Chunk: {:?} {:?}",
            chunk.kind,
            chunk.name,
        );
    }
}

// ── Base64 décodeur RFC 4648 (sans dépendance externe) ─────────────────

/// Décode une chaîne base64 RFC 4648 en bytes.
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for ch in input.chars() {
        let val = match ch {
            'A'..='Z' => (ch as u32) - ('A' as u32),
            'a'..='z' => (ch as u32) - ('a' as u32) + 26,
            '0'..='9' => (ch as u32) - ('0' as u32) + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("Caractère base64 invalide: {ch}")),
        };

        buf = (buf << 6) | val;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            output.push(((buf >> bits) & 0xFF) as u8);
        }
    }

    Ok(output)
}
