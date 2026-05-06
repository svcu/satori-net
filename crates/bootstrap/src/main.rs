use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;

#[derive(Clone)]
struct AppState {
    peers: Arc<RwLock<HashSet<SocketAddr>>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let state = AppState {
        peers: Arc::new(RwLock::new(HashSet::new())),
    };

    let app = Router::new()
        .route("/register", post(register))
        .route("/peers", get(list_peers))
        .with_state(state);

    let addr = "0.0.0.0:1815";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("bootstrap server listening on {addr}");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn register(State(state): State<AppState>, Json(addr): Json<SocketAddr>) -> StatusCode {
    state.peers.write().unwrap().insert(addr);
    tracing::info!("registered peer: {addr}");
    StatusCode::OK
}

#[derive(Deserialize)]
struct PeersQuery {
    addr: SocketAddr,
}

async fn list_peers(
    State(state): State<AppState>,
    Query(query): Query<PeersQuery>,
) -> Json<Vec<SocketAddr>> {
    let peers = state
        .peers
        .read()
        .unwrap()
        .iter()
        .filter(|&&x| x != query.addr)
        .cloned()
        .collect();
    Json(peers)
}
