//! # SHINOBI — Taijutsu : Moteur Central
//!
//! Point d'entrée du backend. Initialise simultanément :
//! - **Axum** : serveur HTTP/REST sur le port configurable
//! - **Tonic** : serveur gRPC (protocole Ninpo) sur le port configurable
//! - **Tensai Consumer** : agent IA consommant les événements Kafka
//!
//! Les trois composants tournent en parallèle dans le même runtime Tokio.
//! Un shutdown gracieux est déclenché via Ctrl+C.

mod config;

use std::net::SocketAddr;
use std::sync::Arc;

use futures::FutureExt;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use application::use_cases::analyze_operation::AnalyzeOperationUseCase;
use application::use_cases::create_operation::CreateOperationUseCase;
use application::use_cases::create_pat::CreatePatUseCase;
use application::use_cases::create_repository::CreateRepositoryUseCase;
use application::use_cases::create_service_account::CreateServiceAccountUseCase;
use application::use_cases::delete_repository::DeleteRepositoryUseCase;
use application::use_cases::get_blob::GetBlobUseCase;
use application::use_cases::get_ipfs_content::GetIpfsContentUseCase;
use application::use_cases::get_operation::GetOperationUseCase;
use application::use_cases::get_operation_diff::GetOperationDiffUseCase;
use application::use_cases::get_reviews::GetReviewsUseCase;
use application::use_cases::get_score_history::GetScoreHistoryUseCase;
use application::use_cases::get_tree::GetTreeUseCase;
use application::use_cases::import_github_repo::ImportGitHubRepoUseCase;
use application::use_cases::list_operations::ListOperationsUseCase;
use application::use_cases::list_refs::ListRefsUseCase;
use application::use_cases::list_repositories::ListRepositoriesUseCase;
use application::use_cases::login_actor::LoginActorUseCase;
use application::use_cases::register_actor::RegisterActorUseCase;
use application::use_cases::resolve_repo::ResolveRepoUseCase;
use application::use_cases::review_operation::ReviewOperationUseCase;
use application::use_cases::search_chunks::SearchChunksUseCase;
use application::use_cases::sensei_chat::SenseiChatUseCase;
use domain::entities::actor::{DEFAULT_REPO_ID, SYSTEM_ACTOR_ID};
use domain::ports::repository::OperationRepository as _; // Trait import — rend list_recent() visible (backfill)
use domain::ports::vcs_engine::VcsEngine as _; // Trait import — rend init_workspace() visible
use infrastructure::auth::jwt_auth_service::JwtAuthService;
use infrastructure::cache::redis_cache::RedisCache;
use infrastructure::content::ipfs_store::IpfsContentStore;
use infrastructure::embeddings::nomic_service::NomicEmbedService;
use infrastructure::events::kafka_consumer::KafkaEventConsumer;
use infrastructure::events::kafka_producer::KafkaEventPublisher;
use infrastructure::events::oracle_consumer::OracleKafkaConsumer;
use infrastructure::github::github_client::GitHubClient;
use infrastructure::llm::ollama_service::OllamaService;
use infrastructure::persistence::postgres_actor_repo::PostgresActorRepository;
use infrastructure::persistence::postgres_federation_repo::PostgresFederationRepository;
use infrastructure::persistence::postgres_chunk_repo::PostgresChunkRepository;
use infrastructure::persistence::postgres_repo::PostgresOperationRepository;
use infrastructure::persistence::postgres_repo_repo::PostgresRepoRepository;
use infrastructure::persistence::postgres_review_repo::PostgresReviewRepository;
use infrastructure::vcs::git_cgi::GitCgiBackend;
use infrastructure::vcs::jujutsu_engine::JujutsuEngine;
use presentation::grpc::services::ShinobiServiceImpl;
use presentation::grpc::services::proto::shinobi_service_server::ShinobiServiceServer;
use presentation::rest::git_http::create_git_router;
use presentation::rest::routes::create_router;
use presentation::state::{GitHttpState, SharedState};
use tensai::multi_chunker::MultiChunker;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Charger .env (silencieux si absent) ───────
    dotenvy::dotenv().ok();

    // ── Observabilité ──────────────────────────────
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();

    print_banner();

    // ── Configuration ──────────────────────────────
    let config = Config::from_env();
    info!(
        rest_port = config.rest_port,
        grpc_port = config.grpc_port,
        vcs_root = %config.vcs_workspace_root,
        kafka_brokers = %config.kafka_brokers,
        ipfs_api_url = %config.ipfs_api_url,
        tensai_enabled = config.tensai_consumer_enabled,
        embedding_enabled = config.embedding_enabled,
        embedding_dimensions = config.embedding_dimensions,
        ollama_url = %config.ollama_url,
        ollama_model = %config.ollama_model,
        oracle_enabled = config.oracle_consumer_enabled,
        sensei_enabled = config.sensei_enabled,
        sensei_ollama_url = %config.sensei_ollama_url,
        sensei_ollama_model = %config.sensei_ollama_model,
        "Configuration chargée"
    );

    // ── Token de shutdown gracieux ─────────────────
    let cancel_token = CancellationToken::new();

    // ── Infrastructure (adaptateurs secondaires) ───
    // Fūinjutsu: PostgreSQL
    let pg_pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;
    info!("✅ PostgreSQL connecté");

    // Auto-migration : applique les migrations SQL pendantes au démarrage.
    // Garantit qu'un volume PostgreSQL vierge (premier `docker compose up`)
    // est automatiquement provisionné sans intervention manuelle.
    sqlx::migrate!("./migrations").run(&pg_pool).await?;
    info!("✅ Migrations SQL appliquées");

    // Fūinjutsu: Redis
    let redis_cache = RedisCache::connect(&config.redis_url).await?;
    info!("✅ Redis connecté");

    // VCS Engine (Anti-Corruption Layer) — auto-init au démarrage
    // Phase 21 : SYSTEM_ACTOR_ID comme propriétaire du DEFAULT_REPO_ID.
    let vcs_engine = JujutsuEngine::new(&config.vcs_workspace_root);
    vcs_engine
        .init_workspace(&SYSTEM_ACTOR_ID, &DEFAULT_REPO_ID)
        .await?;
    info!(
        workspace = %config.vcs_workspace_root,
        owner_id = %SYSTEM_ACTOR_ID,
        repo_id = %DEFAULT_REPO_ID,
        "✅ VCS Engine initialisé (jj-lib ACL — Phase 21 Multi-Tenant)"
    );

    // Nen: Kafka Event Publisher (optionnel — graceful degradation)
    let event_publisher: Option<Arc<dyn domain::ports::event_publisher::EventPublisher>> =
        match KafkaEventPublisher::new(
            &config.kafka_brokers,
            &config.kafka_topic,
            &config.kafka_analysis_topic,
        ) {
            Ok(publisher) => {
                info!(
                    brokers = %config.kafka_brokers,
                    topic = %config.kafka_topic,
                    "✅ Kafka Event Publisher initialisé"
                );
                Some(Arc::new(publisher))
            }
            Err(e) => {
                warn!("⚠️ Kafka non disponible — événements désactivés: {e}");
                None
            }
        };

    // Genjutsu: IPFS Content Store (optionnel — graceful degradation)
    let content_store: Option<Arc<dyn domain::ports::content_store::ContentStore>> =
        match IpfsContentStore::new(&config.ipfs_api_url) {
            Ok(store) => {
                info!(
                    api_url = %config.ipfs_api_url,
                    "✅ IPFS Content Store initialisé (Genjutsu)"
                );
                Some(Arc::new(store))
            }
            Err(e) => {
                warn!("⚠️ IPFS non disponible — stockage distribué désactivé: {e}");
                None
            }
        };

    // ── Adaptateurs ────────────────────────────────────────
    let repo = Arc::new(PostgresOperationRepository::new(pg_pool.clone()));
    let chunk_repo: Arc<dyn domain::ports::chunk_repository::ChunkRepository> =
        Arc::new(PostgresChunkRepository::new(pg_pool.clone()));
    let review_repo: Arc<dyn domain::ports::review_repository::ReviewRepository> =
        Arc::new(PostgresReviewRepository::new(pg_pool.clone()));
    let vcs = Arc::new(vcs_engine);

    info!("✅ ChunkRepository PostgreSQL initialisé (Mémoire IA)");
    info!("✅ ReviewRepository PostgreSQL initialisé (Oracle Reviews)");

    // RAG: Embedding Service Nomic (Phase 7A — optionnel)
    let embedding_service: Option<Arc<dyn domain::ports::embedding_service::EmbeddingService>> =
        if config.embedding_enabled {
            match NomicEmbedService::new(config.embedding_dimensions) {
                Ok(service) => {
                    info!(
                        model = "nomic-embed-text-v1.5",
                        dimensions = config.embedding_dimensions,
                        "✅ EmbeddingService initialisé (Nomic Matryoshka ONNX)"
                    );
                    Some(Arc::new(service))
                }
                Err(e) => {
                    warn!("⚠️ EmbeddingService non disponible — RAG désactivé: {e}");
                    None
                }
            }
        } else {
            info!("ℹ️ EmbeddingService désactivé par configuration (EMBEDDING_ENABLED=false)");
            None
        };

    // Oracle: Ollama LLM Service (Phase 9 — optionnel)
    let llm_service: Option<Arc<dyn domain::ports::llm_service::LlmService>> =
        match OllamaService::new(&config.ollama_url, &config.ollama_model) {
            Ok(service) => {
                info!(
                    url = %config.ollama_url,
                    model = %config.ollama_model,
                    "✅ OllamaService initialisé (LLM local)"
                );
                Some(Arc::new(service))
            }
            Err(e) => {
                warn!("⚠️ Ollama non disponible — Oracle Reviews désactivé: {e}");
                None
            }
        };

    // ── Use Cases (couche application) ───────────
    let create_operation = Arc::new(CreateOperationUseCase::new(
        vcs.clone(),
        repo.clone(),
        event_publisher.clone(),
        content_store.clone(),
    ));
    let get_operation = Arc::new(GetOperationUseCase::new(repo.clone()));
    let list_operations = Arc::new(ListOperationsUseCase::new(repo.clone()));
    let search_chunks = Arc::new(SearchChunksUseCase::new(
        chunk_repo.clone(),
        embedding_service.clone(),
    ));

    // ── Mode CLI Backfill (Phase 7B) ─────────────
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "backfill" {
        return run_backfill(
            repo,
            content_store,
            chunk_repo,
            embedding_service,
            event_publisher,
        )
        .await;
    }

    // ── État partagé (DI Container) ────────────────
    let get_operation_diff = Arc::new(GetOperationDiffUseCase::new(
        repo.clone(),
        vcs.clone(),
        content_store.clone(),
    ));

    let get_ipfs_content = Arc::new(GetIpfsContentUseCase::new(
        repo.clone(),
        content_store.clone(),
    ));

    let get_reviews = Arc::new(GetReviewsUseCase::new(review_repo.clone()));

    let get_score_history = Arc::new(GetScoreHistoryUseCase::new(review_repo.clone()));

    // ── Phase 10C: Résolution sémantique des dépôts ────
    let actor_repo: Arc<PostgresActorRepository> =
        Arc::new(PostgresActorRepository::new(pg_pool.clone()));
    let repo_repo: Arc<PostgresRepoRepository> =
        Arc::new(PostgresRepoRepository::new(pg_pool.clone()));
    let resolve_repo = Arc::new(ResolveRepoUseCase::new(
        actor_repo.clone(),
        repo_repo.clone(),
    ));

    // ── Phase 10D: Création de dépôts (Big Bang) ────
    let list_repositories = Arc::new(ListRepositoriesUseCase::new(
        actor_repo.clone(),
        repo_repo.clone(),
    ));
    let create_repository = Arc::new(CreateRepositoryUseCase::new(
        actor_repo.clone(),
        repo_repo.clone(),
        vcs.clone(),
    ));

    // ── Phase 19A : Auth & RBAC ────────────────────────────────
    let auth_service: Arc<dyn domain::ports::auth_service::AuthService> = Arc::new(
        JwtAuthService::new(&config.jwt_secret, config.jwt_duration_secs),
    );

    let register_actor = Arc::new(RegisterActorUseCase::new(
        actor_repo.clone(),
        auth_service.clone(),
        config.jwt_duration_secs,
    ));
    let login_actor = Arc::new(LoginActorUseCase::new(
        actor_repo.clone(),
        auth_service.clone(),
        config.jwt_duration_secs,
    ));
    let create_pat = Arc::new(CreatePatUseCase::new(
        actor_repo.clone(),
        auth_service.clone(),
    ));

    info!(
        jwt_duration_days = config.jwt_duration_secs / 86400,
        "🔐 Auth Service initialisé (JWT HS256 + Argon2 + PAT SHA-256)"
    );

    // ── Phase 6 : Explorateur de Code ─────────────────────────────
    let get_tree = Arc::new(GetTreeUseCase::new(vcs.clone(), resolve_repo.clone()));
    let get_blob = Arc::new(GetBlobUseCase::new(vcs.clone(), resolve_repo.clone()));
    let list_refs_uc = Arc::new(ListRefsUseCase::new(vcs.clone(), resolve_repo.clone()));

    // ── Phase 15 : Agent Sensei (先生) — LLM conversationnel ─────────
    let sensei_chat: Option<Arc<SenseiChatUseCase>> = if config.sensei_enabled {
        match OllamaService::new(&config.sensei_ollama_url, &config.sensei_ollama_model) {
            Ok(sensei_llm) => {
                let use_case = Arc::new(SenseiChatUseCase::new(
                    search_chunks.clone(),
                    review_repo.clone(),
                    Arc::new(sensei_llm),
                    repo.clone(),
                ));
                info!(
                    url = %config.sensei_ollama_url,
                    model = %config.sensei_ollama_model,
                    "🥷 Sensei Agent — Initialisé (Ollama #2)"
                );
                Some(use_case)
            }
            Err(e) => {
                warn!("⚠️ Sensei Agent désactivé — Ollama #2 non disponible: {e}");
                None
            }
        }
    } else {
        info!("ℹ️ Sensei Agent désactivé par configuration (SENSEI_ENABLED=false)");
        None
    };

    // ── Phase 19B : GitHub Import (Le Pont des Mondes) ────────
    let github_service: Arc<dyn domain::ports::github_service::GitHubService> =
        Arc::new(GitHubClient::new());

    let vcs_concrete = Arc::new(JujutsuEngine::new(&config.vcs_workspace_root));

    let import_github_repo = Arc::new(ImportGitHubRepoUseCase::new(
        github_service.clone(),
        actor_repo.clone(),
        repo_repo.clone(),
        vcs_concrete.clone(),
        repo.clone(),
        content_store.clone(),
        event_publisher.clone(),
    ));

    info!("\u{1f30d} GitHub Import Service initialisé (Phase 19B — Le Pont des Mondes)");

    // ── Phase 20 : GitHub OAuth (Les Portes d'Ōtsutsuki) ──────────
    let oauth_github: Option<Arc<application::use_cases::oauth_github::OAuthGitHubUseCase>> =
        match (&config.github_client_id, &config.github_client_secret) {
            (Some(client_id), Some(client_secret)) => {
                let use_case = Arc::new(
                    application::use_cases::oauth_github::OAuthGitHubUseCase::new(
                        actor_repo.clone(),
                        auth_service.clone(),
                        redis_cache.clone(),
                        client_id.clone(),
                        client_secret.clone(),
                        config.jwt_duration_secs,
                    ),
                );
                info!("🔑 GitHub OAuth initialisé (Phase 20 — Les Portes d'Ōtsutsuki)");
                Some(use_case)
            }
            _ => {
                info!("ℹ️ GitHub OAuth désactivé — GITHUB_CLIENT_ID/SECRET non configurés");
                None
            }
        };

    // ── Phase 20B : Le Clonage Massif (GitHub Bulk Import) ──────────
    let list_github_repos = Arc::new(
        application::use_cases::list_github_repos::ListGitHubReposUseCase::new(
            actor_repo.clone(),
            github_service.clone(),
            repo_repo.clone(),
        ),
    );

    let bulk_import_github = Arc::new(
        application::use_cases::bulk_import_github::BulkImportGitHubUseCase::new(
            import_github_repo.clone(),
            repo_repo.clone(),
        ),
    );

    info!("🐙 GitHub Bulk Import initialisé (Phase 20B — Le Clonage Massif)");

    // ── Phase 24 : Soft Delete (Corbeille) ──────────────────────────
    let delete_repository = Arc::new(DeleteRepositoryUseCase::new(
        actor_repo.clone(),
        repo_repo.clone(),
    ));

    let purge_trash = Arc::new(
        application::use_cases::purge_trash::PurgeTrashUseCase::new(
            repo_repo.clone(),
            std::path::PathBuf::from(&config.vcs_workspace_root),
        ),
    );

    info!("🗑️ Corbeille initialisée (Phase 24 — rétention {}s)",
        application::use_cases::purge_trash::TRASH_RETENTION_SECS
    );

    // ── Phase 25 : Service Accounts (L'Acte de Naissance) ──────────────
    let create_service_account = Arc::new(CreateServiceAccountUseCase::new(
        actor_repo.clone(),
        auth_service.clone(),
    ));

    info!("🤖 Service Accounts initialisé (Phase 25 — L'Acte de Naissance)");

    // ── Phase 26A : Merge Requests (Le Katana Croisé) ──────────────────
    let mr_repo: Arc<dyn domain::ports::mr_repository::MrRepository> = Arc::new(
        infrastructure::persistence::postgres_mr_repo::PostgresMrRepository::new(pg_pool.clone()),
    );
    let create_mr = Arc::new(
        application::use_cases::create_mr::CreateMrUseCase::new(
            mr_repo.clone(),
            repo_repo.clone(),
        ),
    );
    let list_mrs = Arc::new(
        application::use_cases::list_mrs::ListMrsUseCase::new(mr_repo.clone()),
    );
    let get_mr = Arc::new(
        application::use_cases::get_mr::GetMrUseCase::new(mr_repo.clone(), vcs.clone()),
    );
    let review_mr = Arc::new(
        application::use_cases::review_mr::ReviewMrUseCase::new(
            mr_repo.clone(),
            repo_repo.clone(),
        ),
    );
    let merge_mr = Arc::new(
        application::use_cases::merge_mr::MergeMrUseCase::new(
            mr_repo.clone(),
            repo_repo.clone(),
            vcs.clone(),
        ),
    );
    let close_mr = Arc::new(
        application::use_cases::close_mr::CloseMrUseCase::new(
            mr_repo.clone(),
            repo_repo.clone(),
        ),
    );
    let mr_diff = Arc::new(
        application::use_cases::mr_diff::MrDiffUseCase::new(mr_repo.clone(), vcs.clone()),
    );

    info!("⚔️ Merge Requests initialisé (Phase 26A — Le Katana Croisé)");

    // ── Phase 28B — ANBU Checkpoints ──────────────────
    let anbu_repo: Arc<dyn domain::ports::anbu_repository::AnbuRepository> = Arc::new(
        infrastructure::persistence::anbu_repo::PostgresAnbuRepository::new(pg_pool.clone()),
    );
    let create_checkpoint = Arc::new(
        application::use_cases::create_checkpoint::CreateCheckpointUseCase::new(
            anbu_repo.clone(),
            content_store.clone(),
        ),
    );
    let list_checkpoints_uc = Arc::new(
        application::use_cases::list_checkpoints::ListCheckpointsUseCase::new(anbu_repo.clone()),
    );
    info!("🥷 ANBU Checkpoints initialisé (Phase 28B)");

    // ── Phase 27 — ForgeFed (Fédération ActivityPub) ──────────────────
    let federation_repo: Arc<dyn domain::ports::federation_repository::FederationRepository> =
        Arc::new(PostgresFederationRepository::new(pg_pool.clone()));

    // Auto-generate instance keypair (SYSTEM_ACTOR_ID) si absente
    if config.federation_enabled {
        if federation_repo.get_keypair(&SYSTEM_ACTOR_ID).await?.is_none() {
            let keypair = infrastructure::federation::crypto::generate_rsa_keypair()
                .map_err(|e| anyhow::anyhow!("Federation keygen failed: {e}"))?;
            let key_id = format!("https://{}/actors/system#main-key", config.federation_domain);
            let fed_kp = domain::entities::federation::FederationKeypair {
                actor_id: SYSTEM_ACTOR_ID,
                public_key_pem: keypair.public_key_pem,
                private_key_pem: keypair.private_key_pem,
                key_id,
                created_at: chrono::Utc::now(),
            };
            federation_repo.save_keypair(&fed_kp).await?;
            info!("🔑 Federation keypair generated for SYSTEM_ACTOR_ID");
        } else {
            info!("🔑 Federation keypair exists for SYSTEM_ACTOR_ID");
        }
        info!(
            domain = %config.federation_domain,
            "🌐 ForgeFed Federation — Activée (Phase 27)"
        );
    } else {
        info!("ℹ️ ForgeFed Federation désactivée (FEDERATION_ENABLED=false)");
    }

    let shared_state = SharedState {
        create_operation,
        get_operation,
        list_operations,
        search_chunks,
        get_operation_diff,
        get_ipfs_content,
        get_reviews,
        get_score_history,
        resolve_repo: resolve_repo.clone(),
        create_repository,
        list_repositories,
        get_tree,
        get_blob,
        list_refs: list_refs_uc,
        sensei_chat,
        sensei_ollama_url: if config.sensei_enabled {
            Some(config.sensei_ollama_url.clone())
        } else {
            None
        },
        // Phase 17 — Diff Colorisé
        vcs_engine: vcs.clone(),
        operation_repo: repo.clone(),
        // Phase 19A — Auth & RBAC
        auth_service: auth_service.clone(),
        actor_repo: actor_repo.clone(),
        repo_repo: repo_repo.clone(),
        register_actor,
        login_actor,
        create_pat,
        // Phase 19B — GitHub Import
        import_github_repo,
        github_service: github_service.clone(),
        // Phase 20 — GitHub OAuth
        oauth_github,
        frontend_url: std::env::var("FRONTEND_URL").ok(),
        // Phase 20B — Le Clonage Massif
        list_github_repos,
        bulk_import_github,
        // Phase 24 — Soft Delete (Corbeille)
        delete_repository,
        // Phase 25 — Service Accounts (L'Acte de Naissance)
        create_service_account,
        // Phase 26A — Merge Requests (Le Katana Croisé)
        mr_repo,
        create_mr,
        list_mrs,
        get_mr,
        review_mr,
        merge_mr,
        close_mr,
        mr_diff,
        // Phase 28B — ANBU Checkpoints
        create_checkpoint,
        list_checkpoints: list_checkpoints_uc,
        // Phase 27 — ForgeFed (Fédération ActivityPub)
        federation_domain: config.federation_domain.clone(),
        federation_enabled: config.federation_enabled,
        federation_repo,
    };

    // ── Git Bridge HTTP (Phase 12A) ────────────────────
    let git_cgi = match GitCgiBackend::new() {
        Ok(cgi) => {
            info!("\u{2705} Git Bridge HTTP actif (git http-backend)");
            Some(Arc::new(cgi))
        }
        Err(e) => {
            warn!("\u{26a0}\u{fe0f} Git Bridge HTTP desactive — git non trouve: {e}");
            None
        }
    };

    // ── Serveur Axum (REST + Git HTTP) ────────────────
    let rest_addr = SocketAddr::from(([0, 0, 0, 0], config.rest_port));
    let rest_router = if let Some(git_cgi) = git_cgi {
        let git_state = GitHttpState {
            resolve_repo: resolve_repo.clone(),
            vcs_engine: vcs.clone(),
            git_cgi,
            event_publisher: event_publisher.clone(),
            operation_repo: repo.clone(),
            content_store: content_store.clone(),
            workspace_root: std::path::PathBuf::from(&config.vcs_workspace_root),
            // Phase 19A-Git — PAT Auth pour Git HTTP
            auth_service: auth_service.clone(),
            actor_repo: actor_repo.clone(),
            repo_repo: repo_repo.clone(),
        };
        create_router(shared_state.clone()).merge(create_git_router(git_state))
    } else {
        create_router(shared_state.clone())
    };
    let rest_listener = TcpListener::bind(rest_addr).await?;

    info!(
        port = config.rest_port,
        "⚡ Axum REST — En écoute sur http://{rest_addr}"
    );

    let rest_server = async {
        axum::serve(rest_listener, rest_router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|e| anyhow::anyhow!("Axum server error: {e}"))
    };

    // ── Phase 24 : Timer de purge automatique (corbeille) ────────────
    {
        let purge = purge_trash.clone();
        let cancel = cancel_token.clone();
        tokio::spawn(async move {
            // Vérifier toutes les 10 minutes (adapté au délai de rétention de 1h)
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
            loop {
                interval.tick().await;
                if cancel.is_cancelled() {
                    info!("🗑️ Purge timer — arrêt demandé");
                    break;
                }
                match purge.execute().await {
                    Ok(n) if n > 0 => info!("🗑️ Purge: {n} dépôt(s) expiré(s) supprimé(s) définitivement"),
                    Ok(_) => {},
                    Err(e) => warn!("⚠️ Purge automatique échouée: {e}"),
                }
            }
        });
        info!("⏱️ Timer de purge automatique démarré (toutes les 10 min)");
    }

    // ── Serveur Tonic (gRPC / Ninpo) ───────────────
    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], config.grpc_port));
    let shinobi_service = ShinobiServiceImpl::new(shared_state);

    info!(
        port = config.grpc_port,
        "⚡ Tonic gRPC (Ninpo) — En écoute sur http://{grpc_addr}"
    );

    let grpc_server = async {
        tonic::transport::Server::builder()
            .add_service(ShinobiServiceServer::new(shinobi_service))
            .serve_with_shutdown(grpc_addr, shutdown_signal())
            .await
            .map_err(|e| anyhow::anyhow!("Tonic server error: {e}"))
    };

    // ── Agent Tensai : Consumer Kafka (optionnel) ──
    let tensai_consumer_handle = if config.tensai_consumer_enabled {
        match (
            &content_store,
            KafkaEventConsumer::new(
                &config.kafka_brokers,
                &config.kafka_topic,
                &config.kafka_consumer_group,
                cancel_token.clone(),
            ),
        ) {
            (Some(cs), Ok(consumer)) => {
                let analyzer = Arc::new(AnalyzeOperationUseCase::new(
                    cs.clone(),
                    Arc::new(MultiChunker::new()),
                    Some(chunk_repo.clone()), // Phase 6B — Persistence des chunks
                    embedding_service.clone(), // Phase 7A — Embedding vectoriel
                    event_publisher.clone(),  // Phase 7B — Re-publication analysis-complete
                ));

                info!(
                    topic = %config.kafka_topic,
                    group = %config.kafka_consumer_group,
                    "🧠 Tensai Agent IA — Consumer Kafka actif"
                );

                let handle = tokio::spawn(async move {
                    let handler: domain::ports::event_consumer::OperationHandler =
                        Box::new(move |operation| {
                            let analyzer = analyzer.clone();
                            async move {
                                match analyzer.execute(&operation).await {
                                    Ok(_outcome) => {
                                        // Le logging est déjà fait dans le use case.
                                    }
                                    Err(e) => {
                                        warn!(
                                            operation_id = %operation.id,
                                            error = %e,
                                            "⚠️ Tensai — Erreur d'analyse sémantique"
                                        );
                                    }
                                }
                            }
                            .boxed()
                        });

                    if let Err(e) = consumer.start(handler).await {
                        error!("❌ Tensai Consumer terminé avec erreur: {e}");
                    }
                });

                Some(handle)
            }
            (None, _) => {
                warn!("⚠️ Tensai Consumer désactivé — IPFS (ContentStore) non disponible");
                None
            }
            (_, Err(e)) => {
                warn!("⚠️ Tensai Consumer désactivé — Kafka non disponible: {e}");
                None
            }
        }
    } else {
        info!("ℹ️ Tensai Consumer désactivé par configuration (TENSAI_CONSUMER_ENABLED=false)");
        None
    };

    // ── Agent Oracle : Consumer Kafka analysis-complete (Phase 9) ──
    let oracle_consumer_handle = if config.oracle_consumer_enabled {
        match (
            &content_store,
            &llm_service,
            OracleKafkaConsumer::new(
                &config.kafka_brokers,
                &config.kafka_analysis_topic, // Écoute le topic analysis-complete
                &config.oracle_consumer_group,
                cancel_token.clone(),
            ),
        ) {
            (Some(cs), Some(llm), Ok(consumer)) => {
                let reviewer = Arc::new(ReviewOperationUseCase::new(
                    repo.clone(),
                    vcs.clone(),
                    llm.clone(),
                    review_repo.clone(),
                    cs.clone(),
                ));

                info!(
                    topic = %config.kafka_analysis_topic,
                    group = %config.oracle_consumer_group,
                    model = %config.ollama_model,
                    "🔮 Oracle Reviewer — Consumer Kafka actif"
                );

                let handle = tokio::spawn(async move {
                    let handler: infrastructure::events::oracle_consumer::OracleHandler = Box::new(
                        move |operation_id| {
                            let reviewer = reviewer.clone();
                            async move {
                                match reviewer.execute(operation_id).await {
                                    Ok(application::use_cases::review_operation::ReviewOutcome::Reviewed { review_id, duration_ms, .. }) => {
                                        info!(
                                            operation_id = %operation_id,
                                            review_id = %review_id,
                                            duration_ms,
                                            "🔮 Oracle — Review produite avec succès"
                                        );
                                    }
                                    Ok(application::use_cases::review_operation::ReviewOutcome::Skipped { reason, .. }) => {
                                        info!(
                                            operation_id = %operation_id,
                                            reason = %reason,
                                            "🔮 Oracle — Opération ignorée"
                                        );
                                    }
                                    Err(e) => {
                                        warn!(
                                            operation_id = %operation_id,
                                            error = %e,
                                            "⚠️ Oracle — Erreur de code review"
                                        );
                                    }
                                }
                            }
                            .boxed()
                        },
                    );

                    if let Err(e) = consumer.start(handler).await {
                        error!("❌ Oracle Consumer terminé avec erreur: {e}");
                    }
                });

                Some(handle)
            }
            (None, _, _) => {
                warn!("⚠️ Oracle Consumer désactivé — IPFS (ContentStore) non disponible");
                None
            }
            (_, None, _) => {
                warn!("⚠️ Oracle Consumer désactivé — Ollama (LlmService) non disponible");
                None
            }
            (_, _, Err(e)) => {
                warn!("⚠️ Oracle Consumer désactivé — Kafka non disponible: {e}");
                None
            }
        }
    } else {
        info!("ℹ️ Oracle Consumer désactivé par configuration (ORACLE_CONSUMER_ENABLED=false)");
        None
    };

    // ── Lancement simultané ────────────────────────
    tokio::select! {
        result = rest_server => {
            if let Err(e) = result {
                error!("Serveur REST terminé avec erreur: {e}");
            }
        }
        result = grpc_server => {
            if let Err(e) = result {
                error!("Serveur gRPC terminé avec erreur: {e}");
            }
        }
    }

    // ── Shutdown ────────────────────────────────────
    // Annuler le token pour arrêter les consumers.
    cancel_token.cancel();

    // Attendre la fin du consumer Tensai (si actif).
    if let Some(handle) = tensai_consumer_handle {
        info!("🛑 Attente de l'arrêt du consumer Tensai...");
        let _ = handle.await;
    }

    // Attendre la fin du consumer Oracle (si actif).
    if let Some(handle) = oracle_consumer_handle {
        info!("🛑 Attente de l'arrêt du consumer Oracle...");
        let _ = handle.await;
    }

    info!("Taijutsu — Shutdown complet");
    Ok(())
}

/// Signal de shutdown gracieux (Ctrl+C).
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Impossible d'installer le handler Ctrl+C");
    info!("🛑 Signal d'arrêt reçu — shutdown gracieux en cours...");
}

/// Affiche la bannière de démarrage SHINOBI.
fn print_banner() {
    let banner = r#"
    ╔═══════════════════════════════════════════════════════════════╗
    ║                                                               ║
    ║   ███████╗██╗  ██╗██╗███╗   ██╗ ██████╗ ██████╗ ██╗           ║
    ║   ██╔════╝██║  ██║██║████╗  ██║██╔═══██╗██╔══██╗██║           ║
    ║   ███████╗███████║██║██╔██╗ ██║██║   ██║██████╔╝██║           ║
    ║   ╚════██║██╔══██║██║██║╚██╗██║██║   ██║██╔══██╗██║           ║
    ║   ███████║██║  ██║██║██║ ╚████║╚██████╔╝██████╔╝██║           ║
    ║   ╚══════╝╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝ ╚═════╝ ╚═════╝ ╚═╝           ║
    ║                                                               ║
    ║   ⚙️  TAIJUTSU — Moteur Central v0.9.0                        ║
    ║   ⚡ Ninpo (gRPC) + Axum (REST) + Prometheus                  ║
    ║   🧬 RAG Vectoriel (Nomic-Embed-Text-v1.5 + pgvector)          ║
    ║   🧠 Tensai Polyglotte — Rust·TS·TSX·CSS·Python                ║
    ║   🔮 Oracle Reviewer — Code Review IA (Ollama)                 ║
    ║   🥷 Next-Gen VCS for Human/AI Collaboration                   ║
    ║                                                               ║
    ╚═══════════════════════════════════════════════════════════════╝
    "#;
    println!("{banner}");
}

// ── Backfill CLI (Phase 7B) ────────────────────────────────────────
//
// Re-analyse toutes les opérations existantes avec le pipeline d'embedding.
// Idempotent : delete_by_operation() est appelé avant save_chunks().
//
// Usage : cargo run -- backfill

async fn run_backfill(
    repo: Arc<PostgresOperationRepository>,
    content_store: Option<Arc<dyn domain::ports::content_store::ContentStore>>,
    chunk_repo: Arc<dyn domain::ports::chunk_repository::ChunkRepository>,
    embedding_service: Option<Arc<dyn domain::ports::embedding_service::EmbeddingService>>,
    event_publisher: Option<Arc<dyn domain::ports::event_publisher::EventPublisher>>,
) -> anyhow::Result<()> {
    info!("\n📦 BACKFILL MODE — Re-analyse de toutes les opérations existantes");

    let cs = match content_store {
        Some(cs) => cs,
        None => {
            error!("❌ Backfill impossible — IPFS (ContentStore) non disponible");
            return Ok(());
        }
    };

    // Charger toutes les opérations existantes du dépôt par défaut.
    let operations = repo.list_recent(&DEFAULT_REPO_ID, 10_000).await?;
    let total = operations.len();

    info!(total_operations = total, "Opérations chargées");

    let analyzer = AnalyzeOperationUseCase::new(
        cs,
        Arc::new(MultiChunker::new()),
        Some(chunk_repo),
        embedding_service,
        event_publisher,
    );

    let mut analyzed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    for (i, operation) in operations.iter().enumerate() {
        let progress = format!("[{}/{}]", i + 1, total);

        match analyzer.execute(operation).await {
            Ok(application::use_cases::analyze_operation::AnalysisOutcome::Analyzed(report)) => {
                info!(
                    progress = %progress,
                    operation_id = %operation.id,
                    chunks = report.total_chunks,
                    duration_ms = report.duration_ms,
                    "✅ Backfill — Opération analysée"
                );
                analyzed += 1;
            }
            Ok(application::use_cases::analyze_operation::AnalysisOutcome::Skipped {
                reason,
                ..
            }) => {
                info!(
                    progress = %progress,
                    operation_id = %operation.id,
                    reason = %reason,
                    "⏭️ Backfill — Opération skip"
                );
                skipped += 1;
            }
            Err(e) => {
                warn!(
                    progress = %progress,
                    operation_id = %operation.id,
                    error = %e,
                    "⚠️ Backfill — Erreur"
                );
                errors += 1;
            }
        }
    }

    info!(total, analyzed, skipped, errors, "\n🏁 BACKFILL TERMINÉ");

    Ok(())
}
