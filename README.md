# Tashi Vertex

[![Crates.io](https://img.shields.io/crates/v/tashi-vertex)](https://crates.io/crates/tashi-vertex)
[![docs.rs](https://img.shields.io/docsrs/tashi-vertex)](https://docs.rs/tashi-vertex)
[![License](https://img.shields.io/badge/license-BUSL--1.1-blue)](./LICENSE)

Rust bindings for **Tashi Vertex**, an embedded Byzantine fault-tolerant consensus engine based on the [Hashgraph] algorithm.

Tashi Vertex uses a DAG (Directed Acyclic Graph) of cryptographically signed events and virtual voting to achieve consensus finality in under 100 milliseconds — without exchanging explicit vote messages. For a detailed explanation, see the [Vertex whitepaper].

[Hashgraph]: https://hedera.com/hh-ieee_coins_paper-200516.pdf
[Vertex whitepaper]: https://docs.tashi.network/whitepaper/whitepaper/technical-appendices/consensus-protocol

## Features

- **Fast BFT Consensus** — sub-100ms finality with tolerance up to `f = ⌊(n-1)/3⌋` Byzantine participants
- **Async-first** — socket binding and message receiving are `Future`-based
- **Zero runtime dependencies** — only links dynamically to the `tashi-vertex` C library
- **Safe FFI** — opaque pointer wrappers with automatic cleanup via `Drop`
- **Configurable** — 15+ tunable engine parameters (heartbeat, latency thresholds, epoch sizing, etc.)
- **Quorum membership** — query the active voting creators and vote to re-admit recovered nodes
- **Base58 utilities** — encode/decode keys and binary data

## Installation

Add the crate to your project:

```sh
cargo add tashi-vertex
```

### Build Requirements

- **CMake** >= 4.2.3
- The **Tashi Vertex** shared library (`libtashi-vertex.so` / `.dylib` / `.dll`), fetched automatically by the build script

## Quick Start

Generate a keypair for your node:

```rust
use tashi_vertex::KeySecret;

let secret = KeySecret::generate();
let public = secret.public();

println!("Secret: {secret}"); // Base58-encoded DER
println!("Public: {public}");
```

Run a minimal consensus network:

```rust
use tashi_vertex::{Context, Engine, KeySecret, Message, Options, Peers, Socket, Transaction};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key: KeySecret = "BASE58_SECRET_KEY".parse()?;

    // Configure peers in the network
    let mut peers = Peers::new()?;
    peers.insert("127.0.0.1:9001", &"BASE58_PEER_PUBLIC_KEY".parse()?, Default::default())?;
    peers.insert("127.0.0.1:9000", &key.public(), Default::default())?;

    // Initialize the runtime and bind a socket
    let context = Context::new()?;
    let socket = Socket::bind(&context, "127.0.0.1:9000").await?;
    let joining_running_session = false; // new session

    // Start the consensus engine
    let options = Options::default();
    let engine = Engine::start(&context, socket, options, &key, peers, joining_running_session)?;

    // Send a transaction
    let data = b"hello world";
    let mut tx = Transaction::allocate(data.len());
    tx.copy_from_slice(data);
    engine.send_transaction(tx)?;

    // Receive consensus-ordered messages
    while let Some(message) = engine.recv_message().await? {
        match message {
            Message::Event(event) => {
                for tx in event.transactions() {
                    println!("tx: {:?}", tx);
                }
            }
            Message::SyncPoint(_) => { /* session management */ }
        }
    }

    Ok(())
}
```

## Joining or Rejoining a Session

`Engine::start` takes a `joining_running_session: bool` that tells the engine how
this node enters the network. The wrong value can stop the node from reaching
consensus, or cause it to start a session that diverges from the live one, so
choose it deliberately.

### `false` — start or bootstrap a session

Use `false` when the node is part of the network's initial bootstrap:

- **Single-node start** — the first node brings the network up on its own; others
  join later with `true`.
- **Pre-defined address book** — every founding node starts with the same full
  address book and comes up together.

A node started with `false` assumes it is *establishing* the session, not catching
up to one.

### `true` — join or rejoin a running session

Use `true` when the network is already live and this node must catch up to it:

- **Late join** — a brand-new node joining an existing network. **Precondition:** a
  supermajority (2/3 + 1) of current members must have already voted the node in
  with `vote_add_node` (see [Quorum Membership](#quorum-membership)) *before* it
  starts. On start the node requests the latest address book from the peer(s) it
  knows.
- **Rejoin after a crash/restart** — a node that was already a member coming back.
  It reconnects, fetches the current address book, and catches up instead of
  re-bootstrapping.

**Precondition for either case:** the address book must contain at least one
reachable, currently-running peer to fetch state from.

### Failure modes

| Mistake | Result |
|---|---|
| `true` on a genuinely fresh network (no running peer to reach) | Nothing to catch up to; the node makes no progress. |
| `false` when the session is already live (should have rejoined) | The node bootstraps a fresh state and can **diverge** from the live network instead of catching up. |
| `true` late-join without the supermajority add-vote first | The node is not yet a recognized member and cannot be admitted. |

> **Note:** rejoin-after-crash state replay (reconstructing a returning member's
> view) is an area of active hardening. Validate the exact catch-up behaviour
> against the engine version you link against.

## Quorum Membership

The engine exposes the live BFT quorum membership. `active_creators` returns the public keys of the current active voting creators, useful for monitoring or for an external orchestrator that needs ground-truth membership:

```rust
let creators = engine.active_creators()?;
println!("{} active voting creators", creators.len());
```

A node that was kicked (fell behind or failed) can restart with `joining_running_session = true` to sync again, but it does not become a voter until it is re-admitted. Each live voter casts a vote with `vote_add_node`, and once a supermajority of the current voters has voted, the engine re-adds the node to the active set:

```rust
use std::time::Duration;
use tashi_vertex::PeerCapabilities;

engine.vote_add_node(
    "127.0.0.1:9002",
    &"BASE58_PEER_PUBLIC_KEY".parse()?,
    PeerCapabilities::default(),
    Duration::from_secs(60), // how long the vote stays active
)?;
```

The address, public key, and capabilities must describe the creator exactly as originally passed to `Peers::insert`.

## API Overview

| Type | Description |
|---|---|
| [`Engine`] | Starts and drives the consensus engine — send transactions, receive ordered messages, manage quorum membership |
| [`Context`] | Runtime context managing async operations and resources |
| [`Socket`] | Async network socket bound to a local address |
| [`Options`] | Engine configuration (heartbeat, latency, epoch size, threading, etc.) |
| [`Message`] | Received message — either an `Event` or a `SyncPoint` |
| [`Event`] | A finalized event carrying consensus-ordered transactions |
| [`Transaction`] | Allocated buffer for submitting data to the network |
| [`KeySecret`] | Ed25519 secret key for signing (Base58/DER serializable) |
| [`KeyPublic`] | Ed25519 public key for verification (Base58/DER serializable) |
| [`Peers`] | Set of network participants with addresses and capabilities |
| [`SyncPoint`] | Session management decision from the consensus layer |
| [`base58`] | Base58 encoding/decoding utilities |

## Examples

The [`examples/`](./examples) directory contains runnable demos:

- **`key-generate`** — Generate a new keypair
- **`key-parse`** — Parse Base58-encoded keys
- **`pingback`** — Full multi-peer consensus network with transaction exchange

```sh
cargo run --example key-generate
```

### Running the Pingback Example

The `pingback` example runs a 3-node consensus network where each node sends a `PING` transaction. First, generate a keypair for each node:

```sh
cargo run --example key-generate  # run 3 times, save each secret/public key
```

Then start each node in a separate terminal, passing the other two nodes as peers:

```sh
cargo run --example pingback -- \
  -B 127.0.0.1:8001 \
  -K <secret_key> \
  -P <peer2_public_key>@127.0.0.1:8002 \
  -P <peer3_public_key>@127.0.0.1:8003
```

Once all three nodes are running, each will reach consensus and print the ordered events:

```
 :: Configured network for 3 peers
 :: Initialized runtime
 :: Bound local socket
 :: Started the consensus engine
 > Received SYNC POINT
 > Received EVENT
    - From: aSq9DsNNvGhY...
    - Created: 1770174202473826258
    - Consensus: 1770174208954261963
    - Transactions: 1
    - >> PING
 > Received EVENT
    - From: aSq9DsNNvGhY...
    - Created: 1770174208954261966
    - Consensus: 1770174208954261964
    - Transactions: 1
    - >> PING
 > Received EVENT
    - From: aSq9DsNNvGhY...
    - Created: 1770174216230094057
    - Consensus: 1770174208954261965
    - Transactions: 1
    - >> PING
```

## License

This project is licensed under the **Business Source License 1.1** ([`LICENSE`](./LICENSE)). You can use, modify and redistribute the source for non-production purposes such as development, testing and evaluation. Production use requires a commercial license from Tashi Network (https://tashi.network). Each version converts to the Apache License, Version 2.0 four years after it is published.

Versions up to and including 0.16.0-rc2 were released under the Apache License, Version 2.0, and stay available under that license.

### Dynamic Linking to Tashi Vertex

This project dynamically links to the **Tashi Vertex** binary library (proprietary). Its origin and licensing details are provided in the [`NOTICE`](./NOTICE) file.

**Disclaimer:** The Business Source License applies only to the source code of this project. The Tashi Vertex binary is not part of this source distribution and is governed by its own commercial license.
