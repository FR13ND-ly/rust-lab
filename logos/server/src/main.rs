mod db;
mod state;
mod ws;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://logos:secure_password@localhost:5432/logos_db".to_string());
    
    tracing::info!("🔌 Connecting to Database at {}...", db_url);
    
    let state = Arc::new(AppState::new(&db_url).await);

    let app = Router::new()
        .route("/health", get(|| async { "Logos Server OK" }))
        .route("/ws/client", get(ws::ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("🚀 Logos Server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}