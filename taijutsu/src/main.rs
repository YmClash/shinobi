//! # SHINOBI — Taijutsu : Moteur Central
//!
//! Point d'entrée du backend. Initialise simultanément :
//! - **Axum** : serveur HTTP/REST sur le port 3000
//! - **Tonic** : serveur gRPC (protocole Ninpo) sur le port 50051
//!
//! Les deux serveurs tournent en parallèle dans le même runtime Tokio.
//! Un shutdown gracieux est déclenché via Ctrl+C.

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use presentation::grpc::services::proto::shinobi_service_server::ShinobiServiceServer;
use presentation::grpc::services::ShinobiServiceImpl;
use presentation::rest::routes::create_router;

/// Ports de configuration des serveurs.
const REST_PORT: u16 = 3000;
const GRPC_PORT: u16 = 50051;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Observabilité ──────────────────────────────
    // Initialise le subscriber tracing avec filtrage par variable d'env.
    // Usage: RUST_LOG=debug cargo run
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();

    print_banner();

    // ── Serveur Axum (REST) ────────────────────────
    let rest_addr = SocketAddr::from(([0, 0, 0, 0], REST_PORT));
    let rest_router = create_router();
    let rest_listener = TcpListener::bind(rest_addr).await?;

    info!(
        port = REST_PORT,
        "⚡ Axum REST — En écoute sur http://{rest_addr}"
    );

    let rest_server = async {
        axum::serve(rest_listener, rest_router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|e| anyhow::anyhow!("Axum server error: {e}"))
    };

    // ── Serveur Tonic (gRPC / Ninpo) ───────────────
    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], GRPC_PORT));
    let shinobi_service = ShinobiServiceImpl::default();

    info!(
        port = GRPC_PORT,
        "⚡ Tonic gRPC (Ninpo) — En écoute sur http://{grpc_addr}"
    );

    let grpc_server = async {
        tonic::transport::Server::builder()
            .add_service(ShinobiServiceServer::new(shinobi_service))
            .serve_with_shutdown(grpc_addr, shutdown_signal())
            .await
            .map_err(|e| anyhow::anyhow!("Tonic server error: {e}"))
    };

    // ── Lancement simultané ────────────────────────
    // Les deux serveurs tournent en parallèle.
    // Si l'un échoue, l'erreur est propagée immédiatement.
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
    ╔══════════════════════════════════════════════╗
    ║                                              ║
    ║   ███████╗██╗  ██╗██╗███╗   ██╗ ██████╗     ║
    ║   ██╔════╝██║  ██║██║████╗  ██║██╔═══██╗    ║
    ║   ███████╗███████║██║██╔██╗ ██║██║   ██║    ║
    ║   ╚════██║██╔══██║██║██║╚██╗██║██║   ██║    ║
    ║   ███████║██║  ██║██║██║ ╚████║╚██████╔╝    ║
    ║   ╚══════╝╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝ ╚═════╝     ║
    ║                                              ║
    ║   ⚙️  TAIJUTSU — Moteur Central v0.1.0       ║
    ║   ⚡ Ninpo (gRPC) + Axum (REST)              ║
    ║   🥷 Next-Gen VCS for Human/AI Collaboration ║
    ║                                              ║
    ╚══════════════════════════════════════════════╝
    "#;
    println!("{banner}");
}
