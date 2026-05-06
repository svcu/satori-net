# satori-net

> **⚠️ Work in Progress** — this project is under active development and is **not production-ready**. APIs may change without notice, features are incomplete, and the code has not been audited. Use at your own risk.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

A peer-to-peer networking library for Rust with end-to-end encrypted sessions.

Every connection uses an **ephemeral X25519 Diffie-Hellman** handshake followed by **ChaCha20-Poly1305** authenticated encryption, so no plaintext ever crosses the wire. Peer discovery is handled by a lightweight bootstrap server that nodes register with and query on startup.

## Features

- **End-to-end encryption** — X25519 key exchange + ChaCha20-Poly1305 AEAD per session
- **Async-first** — built on [Tokio](https://tokio.rs/) with non-blocking I/O throughout
- **Simple peer discovery** — HTTP bootstrap server for node registration and peer listing
- **Monotonic nonces** — per-direction counters prevent nonce reuse and replay attacks
- **TUN integration** — transparent packet tunneling for VPN-style routing
- **Modular workspace** — core library, bootstrap server, and CLI as separate crates

## Workspace Layout

```
satori-net/
├── crates/
│   ├── net_core/    # Core Node and Session types (library)
│   ├── bootstrap/   # Peer-discovery HTTP server (binary)
│   └── cli/         # Interactive command-line client (WIP)
└── src/
    └── lib.rs       # Workspace re-exports
```

## Quick Start

### Prerequisites

- Rust 1.85+ (edition 2024)
- Linux or Windows (TUN device support required for VPN routing)
- Root / Administrator privileges for TUN device creation

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
# INFO bootstrap server listening on 0.0.0.0:1815
```

### Use the CLI

```sh
# Requires root/admin for TUN device access
sudo cargo run -p cli
```

The interactive menu will prompt for a local port and the bootstrap server URL, then let you list available peers and connect to one as an exit node.

### Library Usage

```rust
use net_core::{Node, Session};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a node with a TUN interface at 10.0.0.1/24
    let mut node = Node::new(1812, "10.0.0.1")?;

    // Bind, register with bootstrap, and start accepting connections
    node.listen("http://localhost:1815").await?;

    // Connect to a known peer
    let peers = node.list_peers().await;
    if let Some(&addr) = peers.first() {
        node.connect(addr).await?;

        // Or open a direct session
        let mut session = Session::connect(addr).await?;
        session.send(b"hello").await?;
        let reply = session.recv().await?;
        println!("received: {:?}", reply);
    }

    Ok(())
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
4. All subsequent messages are AEAD-encrypted with a monotonically incrementing per-direction nonce.

### Framing

Messages are length-prefixed: a 4-byte big-endian `u32` length followed by the payload. This applies to both handshake frames and encrypted data.

### Peer Discovery

Nodes call `POST /register` on the bootstrap server with their public `SocketAddr` and `GET /peers` to receive the current peer list, excluding their own address.

## API Overview

### `Node`

| Method | Description |
|--------|-------------|
| `Node::new(port, tun_ip)` | Create a node bound to `port` with a TUN interface at `tun_ip` |
| `node.listen(bootstrap_url)` | Bind, register with bootstrap, and start accepting connections |
| `node.connect(addr)` | Open an encrypted session to a peer |
| `node.send_to(addr, data)` | Send encrypted bytes to an existing session |
| `node.list_peers()` | Return known peer addresses |
| `node.register(url)` | Manually register with a bootstrap server |
| `node.get_peers(url)` | Manually fetch peers from a bootstrap server |
| `node.tun()` | Return a handle to the underlying TUN device |

### `Session`

| Method | Description |
|--------|-------------|
| `Session::connect(addr)` | Dial a peer and complete the encrypted handshake |
| `session.send(data)` | Encrypt and send bytes |
| `session.recv()` | Receive and decrypt bytes |
| `session.encrypt(plaintext)` | Encrypt without sending (low-level) |
| `session.decrypt(ciphertext)` | Decrypt without receiving (low-level) |
| `session.peer_addr()` | Return the remote peer address |

## Security Considerations

- Keys are **ephemeral per-connection** — there is no long-term identity key yet. Peers cannot authenticate each other's identity.
- The **bootstrap server is unauthenticated** — anyone who can reach it can register and retrieve peers. For any serious deployment, add authentication or run it on a private network.
- This library has **not been audited**. Do not use it for anything security-sensitive.

## Roadmap

- [ ] Persistent node identity (long-term keypairs + signatures)
- [ ] Authenticated bootstrap server
- [ ] NAT traversal / hole punching
- [ ] `no_std` support for embedded targets
- [ ] Structured logging and metrics
- [ ] Test suite (unit + integration)

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

This project is in early development — if you want to contribute, opening an issue to discuss the change first is recommended.

## License

MIT — see [LICENSE](LICENSE).
