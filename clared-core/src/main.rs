use clared_core::{create_router, CedarEngine, SessionManager};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    eprintln!("Clared Guard (Execution Proxy) v0.1.0 starting...");
    eprintln!("  • Mode: In-Memory Cedar DAG + Two-Phase Staging Gateway");
    eprintln!("  • Protocol: JSON-RPC 2.0 (Execution Envelope Spec)");

    let policy_engine = Arc::new(CedarEngine::new().map_err(|e| format!("Failed to load Cedar policies: {}", e))?);
    let session_manager = Arc::new(SessionManager::new(policy_engine));

    let app = create_router(session_manager);

    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string()).parse::<u16>().unwrap_or(4000);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    eprintln!("  • Listening on: http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
