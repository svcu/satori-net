//! Core peer-to-peer networking primitives for satori-net.
//!
//! This crate provides two main types:
//!
//! - [`Node`] — a local network participant that listens for inbound connections,
//!   registers with a bootstrap server, and maintains a table of active sessions.
//! - [`Session`] — an encrypted, bidirectional channel between two peers established
//!   via an ephemeral X25519 Diffie-Hellman handshake and secured with
//!   ChaCha20-Poly1305 AEAD.
//!
//! # Example
//!
//! ```no_run
//! use net_core::{Node, Session};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut node = Node::new(1812, "10.0.0.1")?;
//!     node.listen("http://localhost:1815").await?;
//!
//!     if let Some(&addr) = node.list_peers().await.first() {
//!         let mut session = Session::connect(addr).await?;
//!         session.send(b"hello").await?;
//!     }
//!     Ok(())
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tun_rs::{AsyncDevice, DeviceBuilder};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

/// Errors produced by node and session operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
    #[error("connection closed by peer")]
    ConnectionClosed,
    #[error("invalid handshake frame")]
    InvalidHandshake,
    #[error("no active session for that address")]
    NoSession,
    #[error("TUN device error: {0}")]
    Tun(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// An encrypted point-to-point connection between two peers.
///
/// A `Session` is created either by calling [`Session::connect`] (initiator side)
/// or automatically when a [`Node`] accepts an inbound connection.
///
/// Encryption uses ChaCha20-Poly1305 with monotonically incrementing per-direction
/// nonces, which prevents nonce reuse and replay attacks.
pub struct Session {
    peer_addr: SocketAddr,
    recv_nonce: u64,
    send_nonce: u64,
    cipher: ChaCha20Poly1305,
    stream: TcpStream,
}

/// A local network node that manages peer discovery and inbound sessions.
///
/// Call [`Node::new`] to create a node, then [`Node::listen`] to bind its listening
/// socket, register with the bootstrap server, and start accepting connections in the
/// background.
pub struct Node {
    sessions: Arc<RwLock<HashMap<SocketAddr, Arc<Mutex<Session>>>>>,
    #[allow(dead_code)] // reserved for long-term identity key support
    identity: StaticSecret,
    peers: Arc<RwLock<HashSet<SocketAddr>>>,
    port: u16,
    public_addr: Arc<RwLock<Option<SocketAddr>>>,
    tun: Arc<AsyncDevice>,
}

impl Node {
    /// Creates a new node that will listen on `port` and expose a TUN interface at `tun_ip`.
    ///
    /// The node is not yet bound or registered; call [`listen`](Node::listen) to start it.
    pub fn new(port: u16, tun_ip: &str) -> Result<Self, Error> {
        let tun = DeviceBuilder::new()
            .ipv4(tun_ip, 24, None)
            .build_async()
            .map_err(|e| Error::Tun(e.to_string()))?;

        Ok(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            identity: StaticSecret::random(),
            peers: Arc::new(RwLock::new(HashSet::new())),
            port,
            public_addr: Arc::new(RwLock::new(None)),
            tun: Arc::new(tun),
        })
    }

    /// Returns a handle to the underlying TUN device.
    pub fn tun(&self) -> Arc<AsyncDevice> {
        self.tun.clone()
    }

    /// Opens an encrypted session to `addr` and registers it in the session table.
    pub async fn connect(&mut self, addr: SocketAddr) -> Result<(), Error> {
        let session = Session::connect(addr).await?;
        self.sessions.write().await.insert(addr, Arc::new(Mutex::new(session)));
        self.peers.write().await.insert(addr);
        Ok(())
    }

    /// Encrypts `data` and sends it to the session identified by `addr`.
    pub async fn send_to(&mut self, addr: &SocketAddr, data: &[u8]) -> Result<(), Error> {
        let session = self.sessions.read().await.get(addr).cloned();
        match session {
            Some(s) => s.lock().await.send(data).await,
            None => Err(Error::NoSession),
        }
    }

    /// Returns a snapshot of all currently known peer addresses.
    pub async fn list_peers(&self) -> Vec<SocketAddr> {
        self.peers.read().await.iter().copied().collect()
    }

    /// Registers this node with the bootstrap server at `bootstrap`.
    ///
    /// Discovers the node's public IP via `api.ipify.org`, then sends
    /// `POST /register` with the full `SocketAddr` (public IP + listening port).
    pub async fn register(&mut self, bootstrap: &str) -> Result<(), Error> {
        let client = reqwest::Client::new();

        #[derive(serde::Deserialize)]
        struct IpResponse {
            ip: IpAddr,
        }

        let public_ip = client
            .get("https://api.ipify.org?format=json")
            .send()
            .await?
            .json::<IpResponse>()
            .await?
            .ip;

        let addr = SocketAddr::new(public_ip, self.port);
        *self.public_addr.write().await = Some(addr);

        client
            .post(endpoint(bootstrap, "register"))
            .json(&addr)
            .send()
            .await?;

        Ok(())
    }

    /// Fetches the peer list from the bootstrap server and merges it into the local peer set.
    pub async fn get_peers(&mut self, bootstrap: &str) {
        if let Some(addr) = *self.public_addr.read().await {
            fetch_and_merge_peers(bootstrap, addr, &self.peers).await;
        }
    }

    /// Binds the listening socket, registers with the bootstrap server, and spawns two
    /// background tasks: one that accepts inbound connections and one that refreshes the
    /// peer list from the bootstrap server every 30 seconds.
    pub async fn listen(&mut self, bootstrap: &str) -> Result<(), Error> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&bind_addr).await?;

        if let Err(e) = self.register(bootstrap).await {
            tracing::warn!("bootstrap registration failed: {e}");
        }

        let self_addr = self
            .public_addr
            .read()
            .await
            .unwrap_or_else(|| SocketAddr::new(IpAddr::from([0, 0, 0, 0]), self.port));

        tokio::spawn(accept_loop(listener, self.sessions.clone(), self.tun.clone()));
        tokio::spawn(peer_refresh_loop(bootstrap.to_owned(), self_addr, self.peers.clone()));

        Ok(())
    }
}

async fn fetch_and_merge_peers(
    bootstrap: &str,
    self_addr: SocketAddr,
    peers: &Arc<RwLock<HashSet<SocketAddr>>>,
) {
    let Ok(response) = reqwest::Client::new()
        .get(endpoint(bootstrap, "peers"))
        .query(&[("addr", self_addr.to_string())])
        .send()
        .await
    else {
        return;
    };
    let Ok(addrs) = response.json::<HashSet<SocketAddr>>().await else {
        return;
    };
    peers.write().await.extend(addrs);
}

async fn peer_refresh_loop(
    bootstrap: String,
    self_addr: SocketAddr,
    peers: Arc<RwLock<HashSet<SocketAddr>>>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        fetch_and_merge_peers(&bootstrap, self_addr, &peers).await;
    }
}

async fn accept_loop(
    listener: TcpListener,
    sessions: Arc<RwLock<HashMap<SocketAddr, Arc<Mutex<Session>>>>>,
    tun: Arc<AsyncDevice>,
) {
    loop {
        let Ok((stream, peer_addr)) = listener.accept().await else {
            continue;
        };

        let sessions = sessions.clone();
        let tun = tun.clone();
        tokio::spawn(async move {
            match handshake_incoming(stream, peer_addr).await {
                Ok(session) => {
                    let session = Arc::new(Mutex::new(session));
                    sessions.write().await.insert(peer_addr, session.clone());
                    recv_loop(session, peer_addr, tun).await;
                    sessions.write().await.remove(&peer_addr);
                }
                Err(e) => tracing::error!("handshake failed with {peer_addr}: {e}"),
            }
        });
    }
}

async fn handshake_incoming(mut stream: TcpStream, peer_addr: SocketAddr) -> Result<Session, Error> {
    let key_bytes = read_frame(&mut stream).await?;
    let key_array: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::InvalidHandshake)?;
    let peer_public = PublicKey::from(key_array);

    let local_secret = EphemeralSecret::random();
    let local_public = PublicKey::from(&local_secret);
    let shared = local_secret.diffie_hellman(&peer_public);

    write_frame(&mut stream, local_public.as_bytes()).await?;

    Ok(Session {
        peer_addr,
        recv_nonce: 0,
        send_nonce: 0,
        cipher: ChaCha20Poly1305::new(Key::from_slice(shared.as_bytes())),
        stream,
    })
}

// Receives encrypted packets from a peer and forwards them to the TUN device.
// Outbound routing (TUN → peer) is handled by the CLI's run_tun loop.
async fn recv_loop(session: Arc<Mutex<Session>>, peer_addr: SocketAddr, tun: Arc<AsyncDevice>) {
    loop {
        match session.lock().await.recv().await {
            Ok(data) => {
                if let Err(e) = tun.send(&data).await {
                    tracing::error!("TUN write error from {peer_addr}: {e}");
                    break;
                }
            }
            Err(Error::ConnectionClosed) => {
                tracing::info!("{peer_addr} disconnected");
                break;
            }
            Err(e) => {
                tracing::error!("session error from {peer_addr}: {e}");
                break;
            }
        }
    }
}

impl Session {
    /// Returns the address of the remote peer.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// Encrypts `data` and sends it to the remote peer.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), Error> {
        let ciphertext = self.encrypt(data)?;
        write_frame(&mut self.stream, &ciphertext).await?;
        Ok(())
    }

    /// Receives bytes from the remote peer and decrypts them.
    pub async fn recv(&mut self) -> Result<Vec<u8>, Error> {
        let ciphertext = read_frame(&mut self.stream).await?;
        self.decrypt(ciphertext)
    }

    /// Dials `addr`, performs the X25519 handshake, and returns the established session.
    pub async fn connect(addr: SocketAddr) -> Result<Session, Error> {
        let local_secret = EphemeralSecret::random();
        let local_public = PublicKey::from(&local_secret);

        let mut stream = TcpStream::connect(addr).await?;

        write_frame(&mut stream, local_public.as_bytes()).await?;

        let key_bytes = read_frame(&mut stream).await?;
        let key_array: [u8; 32] = key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| Error::InvalidHandshake)?;
        let peer_public = PublicKey::from(key_array);

        let shared = local_secret.diffie_hellman(&peer_public);

        Ok(Session {
            peer_addr: addr,
            recv_nonce: 0,
            send_nonce: 0,
            cipher: ChaCha20Poly1305::new(Key::from_slice(shared.as_bytes())),
            stream,
        })
    }

    /// Encrypts `plaintext` using the session key and the current outbound nonce counter.
    ///
    /// The counter is incremented on success.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce = counter_nonce(self.send_nonce);
        let ciphertext = self.cipher.encrypt(&nonce, plaintext).map_err(|_| Error::Encrypt)?;
        self.send_nonce += 1;
        Ok(ciphertext)
    }

    /// Decrypts `ciphertext` using the session key and the current inbound nonce counter.
    ///
    /// The counter is incremented on success.
    pub fn decrypt(&mut self, ciphertext: Vec<u8>) -> Result<Vec<u8>, Error> {
        let nonce = counter_nonce(self.recv_nonce);
        let plaintext = self
            .cipher
            .decrypt(&nonce, ciphertext.as_slice())
            .map_err(|_| Error::Decrypt)?;
        self.recv_nonce += 1;
        Ok(plaintext)
    }
}

/// Builds a 12-byte ChaCha20 nonce from a 64-bit counter (little-endian, zero-padded).
fn counter_nonce(count: u64) -> Nonce {
    let mut bytes = [0u8; 12];
    bytes[..8].copy_from_slice(&count.to_le_bytes());
    *Nonce::from_slice(&bytes)
}

/// Appends a path segment to `base`, inserting a `/` separator if needed.
fn endpoint(base: &str, path: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

/// Writes a length-prefixed frame to `stream`: `[u32 big-endian length][payload]`.
pub async fn write_frame<T: AsyncWrite + Unpin>(
    stream: &mut T,
    payload: &[u8],
) -> Result<(), std::io::Error> {
    stream.write_u32(payload.len() as u32).await?;
    stream.write_all(payload).await
}

/// Reads a length-prefixed frame from `stream` and returns the payload.
///
/// Returns [`Error::ConnectionClosed`] when the peer closes the connection cleanly.
pub async fn read_frame<T: AsyncRead + Unpin>(stream: &mut T) -> Result<Vec<u8>, Error> {
    let length = match stream.read_u32().await {
        Ok(n) => n,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(Error::ConnectionClosed);
        }
        Err(e) => return Err(Error::Io(e)),
    };
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload).await.map_err(Error::Io)?;
    Ok(payload)
}
