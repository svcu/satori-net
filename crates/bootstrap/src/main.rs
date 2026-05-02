use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::StatusCode,
    routing::{get, post},
};

#[derive(Clone)]
struct AppState {
    peers: Arc<RwLock<HashSet<SocketAddr>>>,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        peers: Arc::new(RwLock::new(HashSet::new())),
    };

    let app = Router::new()
        .route("/register", post(register))
        .route("/peers", get(list_peers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1815").await.unwrap();
    println!("bootstrap server listening on 0.0.0.0:1815");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

async fn register(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(port): Json<u16>,
) -> StatusCode {
    let peer = SocketAddr::new(addr.ip(), port);
    state.peers.write().unwrap().insert(peer);
    StatusCode::OK
}

async fn list_peers(State(state): State<AppState>) -> Json<Vec<SocketAddr>> {
    let peers = state.peers.read().unwrap().iter().copied().collect();
    Json(peers)
}
