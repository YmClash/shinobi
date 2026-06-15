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
use application::use_cases::get_operation::GetOperationUseCase;
use application::use_cases::list_operations::ListOperationsUseCase;
use application::use_cases::search_chunks::SearchChunksUseCase;
use infrastructure::cache::redis_cache::RedisCache;
use infrastructure::content::ipfs_store::IpfsContentStore;
use infrastructure::embeddings::nomic_service::NomicEmbedService;
use infrastructure::events::kafka_consumer::KafkaEventConsumer;
use infrastructure::events::kafka_producer::KafkaEventPublisher;
use infrastructure::persistence::postgres_chunk_repo::PostgresChunkRepository;
use infrastructure::persistence::postgres_repo::PostgresOperationRepository;
use infrastructure::vcs::jujutsu_engine::JujutsuEngine;
use domain::ports::vcs_engine::VcsEngine as _; // Trait import — rend init_workspace() visible
use presentation::grpc::services::proto::shinobi_service_server::ShinobiServiceServer;
use presentation::grpc::services::ShinobiServiceImpl;
use presentation::rest::routes::create_router;
use presentation::state::SharedState;
use tensai::rust_chunker::RustChunker;

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

    // Fūinjutsu: Redis
    let _redis_cache = RedisCache::connect(&config.redis_url).await?;
    info!("✅ Redis connecté");

    // VCS Engine (Anti-Corruption Layer) — auto-init au démarrage
    // Phase Makimono : évoluer vers un registre dynamique multi-workspace (multi-tenant).
    let vcs_engine = JujutsuEngine::new(&config.vcs_workspace_root);
    vcs_engine.init_workspace("default").await?;
    info!(
        workspace = %config.vcs_workspace_root,
        "✅ VCS Engine initialisé (jj-lib ACL — workspace 'default')"
    );

    // Nen: Kafka Event Publisher (optionnel — graceful degradation)
    let event_publisher: Option<Arc<dyn domain::ports::event_publisher::EventPublisher>> =
        match KafkaEventPublisher::new(&config.kafka_brokers, &config.kafka_topic) {
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
        Arc::new(PostgresChunkRepository::new(pg_pool));
    let vcs = Arc::new(vcs_engine);

    info!("✅ ChunkRepository PostgreSQL initialisé (Mémoire IA)");

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

    // ── Use Cases (couche application) ─────────────
    let create_operation = Arc::new(CreateOperationUseCase::new(
        vcs.clone(),
        repo.clone(),
        event_publisher,
        content_store.clone(),
    ));
    let get_operation = Arc::new(GetOperationUseCase::new(repo.clone()));
    let list_operations = Arc::new(ListOperationsUseCase::new(repo.clone()));
    let search_chunks = Arc::new(SearchChunksUseCase::new(
        chunk_repo.clone(),
        embedding_service.clone(),
    ));

    // ── État partagé (DI Container) ────────────────
    let shared_state = SharedState {
        create_operation,
        get_operation,
        list_operations,
        search_chunks,
    };

    // ── Serveur Axum (REST) ────────────────────────
    let rest_addr = SocketAddr::from(([0, 0, 0, 0], config.rest_port));
    let rest_router = create_router(shared_state.clone());
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
        match (&content_store, KafkaEventConsumer::new(
            &config.kafka_brokers,
            &config.kafka_topic,
            &config.kafka_consumer_group,
            cancel_token.clone(),
        )) {
            (Some(cs), Ok(consumer)) => {
                let analyzer = Arc::new(AnalyzeOperationUseCase::new(
                    cs.clone(),
                    Arc::new(RustChunker::new()),
                    Some(chunk_repo.clone()), // Phase 6B — Persistence des chunks
                    embedding_service.clone(), // Phase 7A — Embedding vectoriel
                    None, // Slot EventPublisher pour re-publication
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
    // Annuler le token pour arrêter le consumer Tensai.
    cancel_token.cancel();

    // Attendre la fin du consumer (si actif).
    if let Some(handle) = tensai_consumer_handle {
        info!("🛑 Attente de l'arrêt du consumer Tensai...");
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
    ║   ⚙️  TAIJUTSU — Moteur Central v0.7.0                        ║
    ║   ⚡ Ninpo (gRPC) + Axum (REST) + Prometheus                  ║
    ║   🧬 RAG Vectoriel (Nomic-Embed-Text-v1.5 + pgvector)          ║
    ║   🧠 Tensai Agent IA — Analyse sémantique temps réel           ║
    ║   🥷 Next-Gen VCS for Human/AI Collaboration                   ║
    ║                                                               ║
    ╚═══════════════════════════════════════════════════════════════╝
    "#;
    println!("{banner}");
}
