# satori-net

[![CI](https://github.com/svcu2310/satori-net/actions/workflows/ci.yml/badge.svg)](https://github.com/svcu2310/satori-net/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

A peer-to-peer networking library for Rust with end-to-end encrypted sessions.

Every connection uses an **ephemeral X25519 Diffie-Hellman** handshake followed by **ChaCha20-Poly1305** authenticated encryption, so no plaintext ever crosses the wire. Peer discovery is handled by a lightweight bootstrap server that nodes register with and query on startup.

## Features

- **End-to-end encryption** — X25519 key exchange + ChaCha20-Poly1305 AEAD per session
- **Async-first** — built on [Tokio](https://tokio.rs/) with non-blocking I/O throughout
- **Simple peer discovery** — HTTP bootstrap server for node registration and peer listing
- **Monotonic nonces** — per-direction counters prevent nonce reuse and replay attacks
- **Modular workspace** — core library, bootstrap server, node binary, and CLI as separate crates

## Workspace Layout

```
satori-net/
├── crates/
│   ├── net_core/    # Core Node and Session types (library)
│   ├── bootstrap/   # Peer-discovery HTTP server (binary)
│   ├── node/        # Node daemon binary (WIP)
│   └── cli/         # Command-line interface (WIP)
└── src/
    └── lib.rs       # Workspace re-exports
```

## Quick Start

### Prerequisites

- Rust 1.85+ (edition 2024)

### Build

```sh
git clone https://github.com/svcu2310/satori-net.git
cd satori-net
cargo build --workspace
```

### Run the Bootstrap Server

The bootstrap server lets nodes find each other. Start it before launching any nodes:

```sh
cargo run -p bootstrap
# Listening on 0.0.0.0:1815
```

### Connect Two Nodes

```rust
use net_core::{Node, Session};

#[tokio::main]
async fn main() {
    // Start node A on port 1812
    let mut node_a = Node::new(1812);
    node_a.listen().await; // binds, registers with bootstrap, fetches peers

    // Start node B on port 1813
    let mut node_b = Node::new(1813);
    node_b.listen().await;

    // Node B opens an encrypted session to a known peer
    let peers = node_b.list_peers().await;
    if let Some(&addr) = peers.first() {
        let mut session = Session::connect(addr).await.expect("connection failed");
        session.send(b"hello from node B").await.unwrap();
    }
}
```

## How It Works

### Handshake

```
Initiator                          Responder
   |                                   |
   |-- [ephemeral public key] -------->|
   |                                   |  derive shared secret
   |<-- [ephemeral public key] --------|
   |                                   |
   |===== ChaCha20-Poly1305 traffic ===|
```

1. The initiator sends its ephemeral X25519 public key.
2. The responder generates its own ephemeral keypair, completes the DH exchange, and replies with its public key.
3. Both sides derive the same 256-bit shared secret and construct a `ChaCha20Poly1305` cipher.
4. All subsequent messages are AEAD-encrypted with a monotonically incrementing nonce.

### Framing

Messages are length-prefixed: a 4-byte big-endian `u32` length followed by the payload. This applies to both handshake frames and encrypted data.

### Peer Discovery

Nodes call `POST /register` on the bootstrap server (passing their listening port) and `GET /peers` to receive the current peer list. The bootstrap server records the caller's IP address and stores `(ip, port)` as a `SocketAddr`.

## API Overview

### `Node`

| Method | Description |
|--------|-------------|
| `Node::new(port)` | Create a node bound to `port` |
| `node.listen()` | Bind, register with bootstrap, start accepting connections |
| `node.list_peers()` | Return known peer addresses |
| `node.register(url)` | Manually register with a bootstrap server |
| `node.get_peers(url)` | Manually fetch peers from a bootstrap server |

### `Session`

| Method | Description |
|--------|-------------|
| `Session::connect(addr)` | Dial a peer and complete the encrypted handshake |
| `session.send(data)` | Encrypt and send bytes |
| `session.recv()` | Receive and decrypt bytes |
| `session.encrypt(plaintext)` | Encrypt without sending (low-level) |
| `session.decrypt(ciphertext)` | Decrypt without receiving (low-level) |

## Security Considerations

- Keys are ephemeral per-connection — there is no long-term identity key yet.
- The bootstrap server is unauthenticated; anyone who can reach it can register and retrieve peers. For production use, add authentication or run it on a private network.
- This library has **not** been audited. Use in production at your own risk.

## Roadmap

- [ ] Persistent node identity (long-term keypairs + signatures)
- [ ] Authenticated bootstrap server
- [ ] NAT traversal / hole punching
- [ ] `node` binary — persistent daemon with config file
- [ ] `cli` — connect, send, and inspect sessions from the terminal
- [ ] `no_std` support for embedded targets

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
