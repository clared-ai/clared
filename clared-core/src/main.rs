use clared_core::{create_router, AdapterRegistry, CapabilitySigner, CedarEngine, SessionManager};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    eprintln!("Clared Guard (reference simulator) v0.1.0 starting...");
    eprintln!("  • Backend: in-memory simulation; no live external writes");
    eprintln!("  • Protocol: JSON-RPC 2.0 (Clared Execution Envelope)");

    let delegation_secret = std::env::var("CLARED_DELEGATION_SECRET")
        .map_err(|_| "CLARED_DELEGATION_SECRET is required and must contain at least 32 bytes")?;
    if delegation_secret.len() < 32 {
        return Err("CLARED_DELEGATION_SECRET must contain at least 32 bytes".into());
    }

    let policy_engine =
        Arc::new(CedarEngine::new().map_err(|e| format!("Failed to load Cedar policies: {}", e))?);
    let signer = Arc::new(CapabilitySigner::random());
    let adapters = Arc::new(
        AdapterRegistry::built_in().map_err(|e| format!("Failed to load adapters: {}", e))?,
    );
    let session_manager = Arc::new(SessionManager::new(
        policy_engine,
        signer.clone(),
        delegation_secret.into_bytes(),
        adapters,
    ));

    let app = create_router(session_manager);

    let port_value = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string());
    let port = port_value
        .parse::<u16>()
        .map_err(|_| format!("PORT must be a valid TCP port, received '{port_value}'"))?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    eprintln!("  • Listening on: http://{}", addr);
    eprintln!("  • Capability signer: {}", signer.public_key_base64());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
