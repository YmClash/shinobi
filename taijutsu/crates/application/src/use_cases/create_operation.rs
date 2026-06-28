//! Use Case: CreateOperation — Créer une nouvelle opération VCS.
//!
//! Orchestre le flux complet (Phase 5 — synchronisation jj ↔ IPFS) :
//! 1. Demande au VcsEngine de créer le changement (→ CID jj-lib)
//! 2. Si `ContentStore` disponible et fichiers non vides →
//!    sérialise en JSON blob et stocke sur IPFS (→ CID IPFS)
//! 3. Construit l'entité Operation avec les deux CID
//! 4. Persiste via le OperationRepository
//! 5. Publie un événement sur le bus (Nen/Kafka) si disponible
//! 6. Épingle le contenu IPFS en background (fire-and-forget)

use std::sync::Arc;

use tracing::{info, warn};
use uuid::Uuid;

use domain::entities::content_id::ContentId;
use domain::entities::operation::Operation;
use domain::errors::DomainError;
use domain::ports::content_store::ContentStore;
use domain::ports::event_publisher::EventPublisher;
use domain::ports::repository::OperationRepository;
use domain::ports::vcs_engine::VcsEngine;

/// Commande d'entrée pour le use case.
#[derive(Debug)]
pub struct CreateOperationCommand {
    pub author_id: Uuid,
    /// Identifiant du dépôt cible (Phase 10B — isolation multi-tenant).
    pub repository_id: Uuid,
    pub description: String,
    pub parent_ids: Vec<Uuid>,
    /// Fichiers à écrire dans le tree du commit.
    /// Vide = commit de métadonnées (empty tree).
    pub files: Vec<(String, Vec<u8>)>,
}

/// Résultat du use case.
#[derive(Debug)]
pub struct CreateOperationResult {
    pub operation: Operation,
}

/// Use case: créer une opération VCS et la persister.
///
/// Reçoit les ports en injection (Arc<dyn Trait>),
/// garantissant l'inversion de dépendance.
///
/// Les ports optionnels (`EventPublisher`, `ContentStore`) permettent
/// le graceful degradation : le use case fonctionne même si Kafka
/// ou IPFS sont indisponibles.
pub struct CreateOperationUseCase {
    vcs_engine: Arc<dyn VcsEngine>,
    repository: Arc<dyn OperationRepository>,
    event_publisher: Option<Arc<dyn EventPublisher>>,
    content_store: Option<Arc<dyn ContentStore>>,
}

impl CreateOperationUseCase {
    /// Construit le use case avec ses dépendances injectées.
    pub fn new(
        vcs_engine: Arc<dyn VcsEngine>,
        repository: Arc<dyn OperationRepository>,
        event_publisher: Option<Arc<dyn EventPublisher>>,
        content_store: Option<Arc<dyn ContentStore>>,
    ) -> Self {
        Self {
            vcs_engine,
            repository,
            event_publisher,
            content_store,
        }
    }

    /// Exécute le cas d'usage.
    ///
    /// ## Flux Phase 5
    /// 1. `VcsEngine.create_operation()` → CID jj-lib (commit hash interne)
    /// 2. Si `ContentStore` disponible + fichiers non vides :
    ///    - Sérialise `files` en blob JSON (`serde_json`)
    ///    - `ContentStore.store(&blob)` → CID IPFS (distribué)
    ///    - Pin IPFS en background (fire-and-forget)
    /// 3. Si IPFS échoue → `warn!`, continue avec `ipfs_content_id = None`
    /// 4. Construit `Operation` avec les deux CID (ou juste jj si IPFS down)
    /// 5. Persiste + publie Kafka
    pub async fn execute(
        &self,
        cmd: CreateOperationCommand,
    ) -> Result<CreateOperationResult, DomainError> {
        // 1. Créer le changement dans le moteur VCS → obtenir le CID jj-lib.
        let parent_id_strings: Vec<String> =
            cmd.parent_ids.iter().map(|id| id.to_string()).collect();

        let content_id = self
            .vcs_engine
            .create_operation(&cmd.repository_id, &cmd.description, &parent_id_strings, &cmd.files)
            .await?;

        info!(
            author_id = %cmd.author_id,
            content_id = %content_id,
            "Opération VCS créée dans le moteur (CID jj-lib)"
        );

        // 2. Synchroniser vers IPFS via Merkle DAG IPLD (Genjutsu) — graceful degradation.
        let ipfs_content_id = self.sync_to_ipfs(&cmd.description, &cmd.files).await;

        // 3. Construire l'entité domaine avec les deux CID.
        let operation = Operation::new(
            cmd.author_id,
            cmd.repository_id,
            content_id,
            ipfs_content_id,
            cmd.description,
            cmd.parent_ids,
        );

        // 4. Persister l'opération.
        self.repository.save(&operation).await?;

        info!(
            operation_id = %operation.id,
            has_ipfs = operation.has_ipfs_content(),
            "Opération persistée avec succès"
        );

        // 5. Publier l'événement sur le bus (Nen) — fire-and-forget.
        // tokio::spawn déplace la publication Kafka en background :
        // la réponse HTTP/gRPC est renvoyée IMMÉDIATEMENT après le save().
        // Le broker Kafka reçoit le message de manière asynchrone.
        // Si la publication échoue, l'opération est déjà persistée (at-most-once).
        if let Some(publisher) = self.event_publisher.clone() {
            let op_clone = operation.clone();
            tokio::spawn(async move {
                if let Err(e) = publisher.publish_operation_created(&op_clone).await {
                    warn!(
                        operation_id = %op_clone.id,
                        error = %e,
                        "Événement non publié (opération persistée malgré tout)"
                    );
                }
            });
        }

        Ok(CreateOperationResult { operation })
    }

    /// Synchronise les fichiers vers IPFS via un Merkle DAG IPLD (Phase 8.1).
    ///
    /// ## Stratégie
    /// Chaque fichier est stocké comme un nœud IPFS indépendant (raw bytes).
    /// Un manifeste DAG-JSON lie tous les fichiers du commit.
    /// IPFS déduplique automatiquement les fichiers identiques entre commits.
    ///
    /// ## Graceful Degradation
    /// - IPFS down → `warn!` + `None` retourné.
    /// - L'opération n'est JAMAIS bloquée par IPFS.
    async fn sync_to_ipfs(
        &self,
        description: &str,
        files: &[(String, Vec<u8>)],
    ) -> Option<ContentId> {
        // Pas de ContentStore ou pas de fichiers → skip
        let store = self.content_store.as_ref()?;
        if files.is_empty() {
            return None;
        }

        // Phase 8.1 — Merkle DAG IPLD (raw bytes, pas de base64)
        match store.store_dag(description, files).await {
            Ok((root_cid, manifest)) => {
                info!(
                    ipfs_cid = %root_cid,
                    file_count = manifest.files.len(),
                    "✅ Merkle DAG IPLD stocké sur IPFS (Genjutsu)"
                );

                // Épingler le manifeste racine en background (fire-and-forget).
                // IPFS résout récursivement et épingle les blocs référencés.
                let store_clone = store.clone();
                let cid_clone = root_cid.clone();
                tokio::spawn(async move {
                    if let Err(e) = store_clone.pin(&cid_clone).await {
                        warn!(
                            ipfs_cid = %cid_clone,
                            error = %e,
                            "⚠️ Pin DAG IPFS échoué (contenu non épinglé)"
                        );
                    }
                });

                Some(root_cid)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "⚠️ Stockage DAG IPFS échoué — opération continue sans CID IPFS"
                );
                None
            }
        }
    }
}

