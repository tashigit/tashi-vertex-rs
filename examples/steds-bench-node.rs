//! swarm-node.rs — Mission-grade Tashi Vertex node for Swarm swarm.
//!
//! Replaces the black-box `drone-comm` binary with a proper bridge that
//! exposes Vertex metadata to the Python orchestrator:
//!
//!   • `event.creator()`      → cryptographic message authorship
//!   • `event.consensus_at()` → consensus timestamp
//!   • `event.created_at()`   → creation timestamp
//!   • `event.hash()`         → deterministic PRNG seed (all nodes identical)
//!   •  Message::SyncPoint    → node kicked/joined at consensus level
//!
//! NOTE: whitened_signature() is NOT called. It segfaults on gossip/heartbeat
//! events that have no famous witnesses yet. event.hash() is a safe direct
//! struct-field read and equally deterministic across all peers.
//!
//! STDIN:  JSON lines → become Vertex Transactions → consensus-ordered
//! STDOUT: structured lines:
//!     AUTONOMOUS_NODE_READY
//!     EVENT:{"creator":"<b58>","created_at":<ns>,"consensus_at":<ns>,"latency_ms":<f>,"whitened_sig":"<hex>","txns":["..."]}
//!     SYNC_POINT:{"info":"topology_change","timestamp":<ns>}
//!
//! CLI:
//!     -B 127.0.0.1:9500       bind address
//!     -K <secret_key>         Ed25519 secret (Base58)
//!     -P <pub@addr>           peers (repeatable). Append :relay for relay nodes.
//!     -R                      relay mode: no_logic forwarder (kickable + votes)
//!     -F <seconds>            fallen_behind_kick_s (default 180)
//!     -H <microseconds>       heartbeat interval (default 500000 = 500ms)
//!     -S                      enable state sharing (default true)
//!     -J                      join an already-running session

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::str::{FromStr, from_utf8};

use anyhow::anyhow;
use clap::Parser;
use tashi_vertex::{
    Context, Engine, KeyPublic, KeySecret, Message, Options, PeerCapabilities, Peers, Socket,
    StateResult, Transaction,
};
// ── CLI argument types ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct PeerArg {
    pub address: String,
    pub public: KeyPublic,
    pub is_relay: bool,
    pub is_no_order: bool,
    pub is_anchor: bool,
}

impl FromStr for PeerArg {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Format: <public_key>@<address>  or  <public_key>@<address>:relay
        //         or  :no_order  or  :anchor (unkickable voting anchor = GCS)
        let (main, is_relay, is_no_order, is_anchor) = if s.ends_with(":relay") {
            (&s[..s.len() - 6], true, false, false)
        } else if s.ends_with(":no_order") {
            (&s[..s.len() - 9], false, true, false)
        } else if s.ends_with(":anchor") {
            (&s[..s.len() - 7], false, false, true)
        } else {
            (s, false, false, false)
        };

        let (public_str, address) = main
            .split_once('@')
            .ok_or_else(|| anyhow!("Invalid peer format, expected <public_key>@<address>"))?;

        let public = public_str.parse()?;
        let address = address.to_string();

        Ok(PeerArg {
            address,
            public,
            is_relay,
            is_no_order,
            is_anchor,
        })
    }
}

#[derive(Debug, Parser)]
#[command(name = "swarm-node", about = "Swarm swarm Vertex consensus node")]
struct Args {
    /// Logical node id (e.g. Drone1, Drone6, GCS)
    #[clap(short = 'N')]
    pub node_id: String,

    /// Bind address (e.g. 127.0.0.1:9500)
    #[clap(short = 'B')]
    pub bind: String,

    /// Ed25519 secret key (Base58)
    #[clap(short = 'K')]
    pub key: String,

    /// Peers: <public_key>@<address> (repeatable). Append :relay for relay nodes.
    #[clap(short = 'P')]
    pub peers: Vec<PeerArg>,

    /// This node is a relay (no_logic silent forwarder; kickable + votes)
    #[clap(short = 'R', long, default_value_t = false)]
    pub relay: bool,

    /// This node is an observer (no_order = does NOT contribute to event ordering / BFT quorum). Use for RECON drones so supermajority loss of drones does not stall consensus on the anchor nodes
    #[clap(long, default_value_t = false)]
    pub no_order: bool,

    /// This node is the unkickable voting ANCHOR (GCS): full voter that can never be voted out, keeping the population from collapsing to drones-only
    #[clap(long, default_value_t = false)]
    pub anchor: bool,

    /// Seconds before voting to kick a fallen-behind peer (spec §1.6 = 180)
    #[clap(short = 'F', long, default_value_t = 180)]
    pub fallen_behind_kick_s: i64,

    /// Heartbeat interval in microseconds (default 500ms)
    #[clap(short = 'H', long, default_value_t = 500_000)]
    pub heartbeat_us: u64,

    /// Enable state sharing for late-join epoch sync
    #[clap(short = 'S', long, default_value_t = true)]
    pub state_sharing: bool,

    /// Join an already-running session (for late rejoin)
    #[clap(short = 'J', long, default_value_t = false)]
    pub join_session: bool,

    /// Report gossip events (internal Vertex heartbeats). Default OFF to prevent
    /// stdout pipe flooding that blocks stdin draining and stalls the mesh.
    #[clap(short = 'G', long, default_value_t = false)]
    pub gossip_events: bool,

    /// Keep the mission summary (MissionState) from the finalized order, report
    /// it to the engine at every epoch, and on rejoin fetch the summary a
    /// supermajority signed instead of the events missed. Prints STATE: lines.
    #[clap(long, default_value_t = false)]
    pub mission_state: bool,
}

// ── Mission summary ─────────────────────────────────────────────────────────
//
// Built only from finalized transactions, in consensus order, so every node
// that saw the same order holds byte-identical state. A node coming back after
// an outage downloads it (checked by the engine against the hash a
// supermajority signed) rather than replaying what it missed.
//
// Transactions are `v1|TYPE=<t>|node=<n>|k=v|...`. Per node and type the latest
// fields are kept (battery, mode, position, target, ...), plus a count.

#[derive(Default)]
struct MissionState {
    txs: u64,
    /// node -> type -> (count, latest fields)
    nodes: BTreeMap<String, BTreeMap<String, (u64, BTreeMap<String, String>)>>,
    other: u64,
}

impl MissionState {
    fn apply(&mut self, tx: &str) {
        self.txs += 1;
        let mut parts = tx.split('|');
        if parts.next() != Some("v1") {
            self.other += 1;
            return;
        }
        let mut kind = None;
        let mut node = None;
        let mut fields = BTreeMap::new();
        for part in parts {
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            match k {
                "TYPE" => kind = Some(v.to_string()),
                "node" => node = Some(v.to_string()),
                _ => {
                    fields.insert(k.to_string(), v.to_string());
                }
            }
        }
        let (Some(kind), Some(node)) = (kind, node) else {
            self.other += 1;
            return;
        };
        let entry = self.nodes.entry(node).or_default().entry(kind).or_default();
        entry.0 += 1;
        entry.1 = fields;
    }

    /// Canonical encoding: sorted, one line per node and type.
    fn encode(&self) -> Vec<u8> {
        let mut out = format!("txs={} other={}\n", self.txs, self.other);
        for (node, kinds) in &self.nodes {
            for (kind, (count, fields)) in kinds {
                out.push_str(&format!("{node}|{kind}|{count}"));
                for (k, v) in fields {
                    out.push_str(&format!("|{k}={v}"));
                }
                out.push('\n');
            }
        }
        out.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let text = from_utf8(bytes).ok()?;
        let mut lines = text.lines();
        let mut state = MissionState::default();
        for kv in lines.next()?.split(' ') {
            match kv.split_once('=')? {
                ("txs", v) => state.txs = v.parse().ok()?,
                ("other", v) => state.other = v.parse().ok()?,
                _ => return None,
            }
        }
        for line in lines {
            let mut parts = line.split('|');
            let node = parts.next()?.to_string();
            let kind = parts.next()?.to_string();
            let count: u64 = parts.next()?.parse().ok()?;
            let fields = parts
                .map(|p| {
                    p.split_once('=')
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                })
                .collect::<Option<BTreeMap<_, _>>>()?;
            state
                .nodes
                .entry(node)
                .or_default()
                .insert(kind, (count, fields));
        }
        Some(state)
    }
}

/// FNV-1a over the encoding: lets the bench compare summaries across nodes.
/// (The engine signs its own cryptographic hash of the same bytes.)
fn digest(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// What arrived while the agreed summary was being downloaded.
enum Held {
    Txs(Vec<String>),
    Epoch(u64),
}

struct StateSync {
    state: MissionState,
    /// Whether this node has a summary it can extend (false until the first
    /// epoch of a fresh session, or until a rejoin download completes).
    seen_epoch: bool,
    joining: bool,
    awaiting: Option<(
        u64,
        std::sync::mpsc::Receiver<StateResult>,
        std::time::Instant,
    )>,
    /// A failed download: fetch again at the next epoch.
    refetch: bool,
    held: Vec<Held>,
}

impl StateSync {
    fn report(&mut self, engine: &Engine, epoch: u64, src: &str) {
        let bytes = self.state.encode();
        if let Err(e) = engine.report_state(epoch, &bytes) {
            eprintln!("STATE_REPORT_ERROR: {e}");
        }
        println!(
            "STATE:{{\"epoch\":{},\"digest\":\"{}\",\"txs\":{},\"bytes\":{},\"src\":\"{}\"}}",
            epoch,
            digest(&bytes),
            self.state.txs,
            bytes.len(),
            src
        );
        let _ = io::stdout().flush();
    }

    fn on_txs(&mut self, txs: Vec<String>) {
        if self.awaiting.is_some() || self.refetch {
            self.held.push(Held::Txs(txs));
            return;
        }
        for tx in &txs {
            self.state.apply(tx);
        }
    }

    fn on_sync_point(&mut self, engine: &Engine, epoch: u64, just_synced: bool) {
        // A reset onto a newer address book (or a join with -J): our summary
        // misses the epochs in between, so take the one the others agreed on.
        let rejoin = just_synced && (self.seen_epoch || self.joining);
        if rejoin || self.refetch {
            self.held.clear();
            self.refetch = false;
            match engine.request_state(epoch) {
                Ok(rx) => {
                    println!("STATE_FETCH:{{\"epoch\":{epoch}}}");
                    let _ = io::stdout().flush();
                    self.awaiting = Some((epoch, rx, std::time::Instant::now()));
                }
                Err(e) => {
                    eprintln!("STATE_FETCH_ERROR: {e}");
                    self.refetch = true;
                }
            }
            return;
        }
        if self.awaiting.is_some() {
            self.held.push(Held::Epoch(epoch));
            return;
        }
        self.seen_epoch = true;
        self.report(engine, epoch, "local");
    }

    /// Called every loop turn; never blocks.
    fn poll(&mut self, engine: &Engine) {
        let Some((epoch, rx, started)) = &self.awaiting else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => StateResult::Failed,
        };
        let (epoch, waited_ms) = (*epoch, started.elapsed().as_millis());
        self.awaiting = None;
        match result {
            StateResult::Ready(bytes) => match MissionState::decode(&bytes) {
                Some(state) => {
                    println!(
                        "STATE_FETCHED:{{\"epoch\":{epoch},\"digest\":\"{}\",\"bytes\":{},\"waited_ms\":{waited_ms}}}",
                        digest(&bytes),
                        bytes.len()
                    );
                    self.state = state;
                }
                None => {
                    eprintln!("STATE_DECODE_ERROR epoch={epoch}");
                    self.refetch = true;
                    return;
                }
            },
            StateResult::Converged => {}
            other => {
                eprintln!("STATE_FETCH_FAILED epoch={epoch}: {other:?}");
                self.refetch = true;
                return;
            }
        }
        self.seen_epoch = true;
        self.report(engine, epoch, "fetched");
        for held in std::mem::take(&mut self.held) {
            match held {
                Held::Txs(txs) => {
                    for tx in &txs {
                        self.state.apply(tx);
                    }
                }
                Held::Epoch(epoch) => self.report(engine, epoch, "local"),
            }
        }
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The engine handle is !Send, so the task that waits on recv_message runs on a LocalSet.
    tokio::task::LocalSet::new().run_until(node_main()).await
}

async fn node_main() -> anyhow::Result<()> {
    let args = Args::parse();
    let key = args.key.parse::<KeySecret>()?;

    // ── Build peer set with role-specific capabilities ──────────────────────
    // Each drone receives ALL peers (including itself) from Python.
    // We must skip duplicates: if a -P entry matches our own bind address or
    // public key, skip it and insert ourselves exactly once below.
    let mut peers = Peers::with_capacity(args.peers.len() + 1)?;

    let self_pub_str = format!("{}", key.public());

    for peer in &args.peers {
        let peer_pub_str = format!("{}", peer.public);
        // Skip self — we insert ourselves after the loop
        if peer.address == args.bind || peer_pub_str == self_pub_str {
            continue;
        }
        let caps = if peer.is_relay {
            // Relay: silent forwarder (no_logic) but kickable + votes.
            // Only the GCS anchor is unkickable.
            PeerCapabilities {
                no_logic: true,
                ..Default::default()
            }
        } else if peer.is_no_order {
            PeerCapabilities {
                no_order: true,
                ..Default::default()
            }
        } else if peer.is_anchor {
            // Unkickable voting anchor (GCS): full voter that can never be
            // voted out, so the population never collapses to drones-only.
            PeerCapabilities {
                unkickable: true,
                ..Default::default()
            }
        } else {
            Default::default()
        };
        peers.insert(&peer.address, &peer.public, caps)?;
    }

    // Insert ourselves with the correct capabilities
    let self_caps = if args.relay {
        // Relay: silent forwarder (no_logic) but kickable + votes.
        // Only the GCS anchor is unkickable.
        PeerCapabilities {
            no_logic: true,
            ..Default::default()
        }
    } else if args.no_order {
        PeerCapabilities {
            no_order: true,
            ..Default::default()
        }
    } else if args.anchor {
        PeerCapabilities {
            unkickable: true,
            ..Default::default()
        }
    } else {
        Default::default()
    };
    peers.insert(&args.bind, &key.public(), self_caps)?;

    // The actual peer count = unique remotes + self
    let remote_count = args
        .peers
        .iter()
        .filter(|p| p.address != args.bind && format!("{}", p.public) != self_pub_str)
        .count();
    eprintln!(
        " :: Configured network for {} peers (relay={}, no_order={})",
        remote_count + 1,
        args.relay,
        args.no_order
    );

    // ── Vertex context + socket ─────────────────────────────────────────────
    let context = Context::new()?;
    let socket = Socket::bind(&context, &args.bind).await?;

    // ── Engine options — match pingback defaults (known-working) ─────────
    //
    // CRITICAL: Do NOT set enable_state_sharing or epoch_states_to_cache.
    // Stress testing proved that state_sharing causes the Vertex C library to
    // deadlock after epoch transitions with 7 nodes. With these removed,
    // consensus survives 3+ SYNC_POINTs continuously.
    //
    // Also do NOT set custom ACK latency thresholds or disable dynamic epoch
    // sizing — both cause the engine to freeze after the first epoch boundary.
    let mut options = Options::default();

    // Gossip events ON so recv_message() delivers finalized consensus events.
    options.set_report_gossip_events(true);

    // Kick tolerance: use CLI arg (default 180 for mission, 10 for pingback)
    options.set_fallen_behind_kick_s(args.fallen_behind_kick_s);

    // REAL-HARDWARE FIX: Disable UDP hole punching.
    // Hole punching defaults to ON and is designed for nodes behind NAT.
    // On a direct LAN (10.223.0.x), the C library stalls probing for
    // external STUN routes that don't exist, which blocks recv_message().await
    // after the first SYNC_POINT and prevents any EVENT or TICK from firing.
    // In SITL (loopback 127.0.0.1) hole punching is a harmless no-op, which
    // is why this never surfaced before real-hardware testing.
    options.set_enable_hole_punching(false);

    // ── Epoch-speed tuning (faster voter-set reconfiguration) ────────────────
    // REAL-HARDWARE NOTE: Use slower cadence than SITL.
    // 100ms heartbeat + 15ms min-event-interval was tuned for loopback where
    // gossip completes in microseconds. On a real LAN (1ms RTT, Pi CPUs),
    // this generates ~67 events/sec per node, overwhelming the Pi's network
    // stack and causing gossip backpressure. 500ms/50ms matches the Tashi
    // Vertex "known-working" defaults and gives real hardware room to breathe.
    options.set_heartbeat_us(500_000); // 500ms heartbeat (Vertex default)
    // 50ms min real-event interval (scaled by node count in the engine).
    // SN_BASE_MIN_EVENT_US overrides it for pace experiments on the bench.
    let base_min_event_us: u64 = std::env::var("SN_BASE_MIN_EVENT_US")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000);
    options.set_base_min_event_interval_us(base_min_event_us);

    // Mission summary: peers may download our state, and the engine waits for
    // our report at each epoch instead of signing an empty state (it keeps one
    // state proof per creator per epoch, so the empty one would win).
    if args.mission_state {
        options.set_enable_state_sharing(true);
        options.set_wait_for_state_report(true);
    }

    // ── Start engine ────────────────────────────────────────────────────────
    let engine = Engine::start(&context, socket, options, &key, peers, args.join_session)?;
    // Lives for the whole process; the recv task below needs a 'static borrow.
    let engine: &'static Engine = Box::leak(Box::new(engine));

    // recv_message() only returns when something is finalized, so while consensus is stalled
    // (a split, a lost voter) awaiting it inline froze stdin draining: app transactions piled
    // up in this process and reached the engine all at once on recovery. The future is awaited
    // to completion in its own task (never dropped mid-flight, which corrupts the C side) and
    // results come back over a channel, which is safe to poll with a timeout.
    let (msg_tx, mut msg_rx) =
        tokio::sync::mpsc::unbounded_channel::<tashi_vertex::Result<Option<Message>>>();
    tokio::task::spawn_local(async move {
        loop {
            let res = engine.recv_message().await;
            let failed = res.is_err();
            if msg_tx.send(res).is_err() || failed {
                break;
            }
        }
    });

    // Send an initial heartbeat transaction immediately to establish DAG presence.
    // Use Pipe v1 so downstream consumers can validate it with the same contract.
    let bootstrap_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Err(e) = send_transaction_cstr(
        &engine,
        &format!(
            "v1|TYPE=HEARTBEAT|node={}|seq=1|battery=100|link=good|mode=boot|ts={}",
            args.node_id, bootstrap_ts
        ),
    ) {
        eprintln!("INIT_PING_ERROR: {}", e);
    }

    // Fast lane (see `mod fast`), on when SN_FAST_PORT is set.
    let mut fast_lane = match std::env::var("SN_FAST_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
    {
        None => None,
        Some(port) => {
            let ip_of = |addr: &str| {
                addr.rsplit_once(':')
                    .and_then(|(h, _)| h.parse::<std::net::IpAddr>().ok())
            };
            let bind_ip =
                ip_of(&args.bind).ok_or_else(|| anyhow!("fast lane: bad bind address"))?;
            let mut keys = Vec::new();
            for p in &args.peers {
                if p.address == args.bind || format!("{}", p.public) == self_pub_str {
                    continue;
                }
                let ip = ip_of(&p.address)
                    .ok_or_else(|| anyhow!("fast lane: bad peer address {}", p.address))?;
                keys.push((ip, fast::verifying_key(&p.public.to_der_vec()?)?));
            }
            let lane = fast::start(bind_ip, port, fast::signing_key(&key.to_der_vec()?)?, keys)?;
            eprintln!(" :: fast lane on udp {}:{}", bind_ip, port);
            Some(lane)
        }
    };

    println!("AUTONOMOUS_NODE_READY");
    let _ = io::stdout().flush();

    // ── STDIN reader → channel → main loop ─────────────────────────────────
    // Engine is !Sync so we can't share it across threads. We read stdin on a
    // blocking thread, push strings via mpsc, and consume in the async loop.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(text) => {
                    let text = text.trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    if tx.send(text).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // ── Main event loop: interleave stdin TX, Vertex RX, and status ticker ──
    //
    // CRITICAL: engine.recv_message() is NOT cancel-safe!
    //
    // The Vertex SDK's MessageRecieve future passes a raw `*mut c_void` pointer
    // to the C library's tv_message_recv() callback. If tokio::select! drops the
    // future before the callback fires (which `biased;` guarantees whenever stdin
    // or tick is ready), the C callback writes into freed memory:
    //
    //   use-after-free → silent engine corruption → events_rx freezes forever
    //
    // The fix: NEVER put recv_message() inside select!. Instead, we:
    //   1. Non-blocking drain stdin channel (try_recv)
    //   2. Check if TICK is due (manual timer)
    //   3. Await recv_message() to COMPLETION (never dropped mid-flight)
    //
    // With 7 nodes + gossip, recv_message returns 50-100x/sec, so stdin
    // latency is <20ms — well within acceptable bounds.

    let mut txs_sent: u64 = 1; // count the boot heartbeat
    let mut txs_err: u64 = 0;
    let mut events_rx: u64 = 0;
    let mut unknown_rx: u64 = 0;
    let start = std::time::Instant::now();
    let mut last_tick = std::time::Instant::now();
    // 2s tick → the GCS sees the active-voter set (ACTIVE: line) refresh within
    // ~2s of a kick, so the kill-gate re-opens promptly after a kick lands.
    let tick_interval = std::time::Duration::from_secs(2);
    let mut leave_at: Option<std::time::Instant> = None;
    let mut mission = args.mission_state.then(|| StateSync {
        state: MissionState::default(),
        seen_epoch: false,
        joining: args.join_session,
        awaiting: None,
        refetch: false,
        held: Vec::new(),
    });

    loop {
        // PHASE 1: Non-blocking drain of ALL pending stdin lines.
        // This prevents stdin pipe backpressure without cancelling recv_message.
        loop {
            match rx.try_recv() {
                Ok(text) => {
                    // Status for the fast lane: signed datagram to every peer, not consensus.
                    if let Some(payload) = text.strip_prefix("FAST:") {
                        if let Some(lane) = fast_lane.as_mut() {
                            lane.send(payload.as_bytes());
                        }
                        continue;
                    }
                    // Control line: re-admit a recovered creator into the voting
                    // set. Format: VOTE_ADD:<base58_pub>|<host:port>|<cap_byte>
                    // (cap bits match peer insertion: bit2 = no_logic/relay).
                    if let Some(rest) = text.strip_prefix("VOTE_ADD:") {
                        let parts: Vec<&str> = rest.splitn(3, '|').collect();
                        if parts.len() == 3 {
                            match parts[0].parse::<KeyPublic>() {
                                Ok(pubkey) => {
                                    let cap: u8 = parts[2].trim().parse().unwrap_or(0);
                                    let caps = tashi_vertex::PeerCapabilities {
                                        no_order: cap & (1 << 1) != 0,
                                        no_logic: cap & (1 << 2) != 0,
                                        public: cap & (1 << 3) != 0,
                                        unkickable: cap & (1 << 4) != 0,
                                    };
                                    match engine.vote_add_node(
                                        parts[1],
                                        &pubkey,
                                        caps,
                                        std::time::Duration::from_secs(30),
                                    ) {
                                        Ok(()) => eprintln!("VOTE_ADD ok: {}", parts[1]),
                                        Err(e) => eprintln!("VOTE_ADD_ERROR: {}", e),
                                    }
                                }
                                Err(e) => eprintln!("VOTE_ADD_PARSE_ERROR: {}", e),
                            }
                        } else {
                            eprintln!("VOTE_ADD_FORMAT_ERROR: {}", rest);
                        }
                        continue; // not a transaction
                    }
                    // Control line: leave the session for good (kamikaze drone
                    // destroyed with its target). The engine votes to remove
                    // itself, which passes on its own vote; we keep pumping
                    // messages until it winds down so the vote goes out.
                    if text.trim() == "LEAVE" {
                        match engine.stop() {
                            Ok(()) => eprintln!("LEAVE: engine stop signalled"),
                            Err(e) => eprintln!("LEAVE_ERROR: {}", e),
                        }
                        leave_at = Some(std::time::Instant::now());
                        continue;
                    }
                    match send_transaction_cstr(&engine, &text) {
                        Ok(()) => {
                            txs_sent += 1;
                        }
                        Err(e) => {
                            txs_err += 1;
                            eprintln!("TX_ERROR: {}", e);
                        }
                    }
                }
                Err(_) => break, // channel empty or disconnected
            }
        }

        // A summary download finishing (non-blocking).
        if let Some(sync) = mission.as_mut() {
            sync.poll(&engine);
        }

        // PHASE 2: Periodic TICK — manual timer check (no async, no cancellation risk).
        if last_tick.elapsed() >= tick_interval {
            println!(
                "TICK:{{\"txs_sent\":{},\"txs_err\":{},\"events_rx\":{},\"unknown_rx\":{},\"uptime_s\":{}}}",
                txs_sent,
                txs_err,
                events_rx,
                unknown_rx,
                start.elapsed().as_secs()
            );
            // Ground-truth BFT quorum membership: the current active voting
            // creators (base58 pubkeys, same format as event.creator()). The
            // Python GCS maps these to drones and uses the set as the exact
            // denominator for its kill-gate. Queried off the hot recv path.
            let active = engine.active_creators().unwrap_or_default();
            let ids: Vec<String> = active.iter().map(|k| format!("{}", k)).collect();
            println!("ACTIVE:{}", ids.join(","));
            let _ = io::stdout().flush();
            last_tick = std::time::Instant::now();
        }

        // Await the next consensus message to completion.
        //
        // MUST NOT be wrapped in tokio::time::timeout or tokio::select! —
        // dropping this future mid-flight corrupts the C library's internal
        // state (triggers 'assertion failed: snapshot.is_join_interested()').
        //
        // With state_sharing disabled (see options above), the engine no
        // longer deadlocks after epoch transitions, so blocking here is safe.
        let next =
            match tokio::time::timeout(std::time::Duration::from_millis(10), msg_rx.recv()).await {
                Ok(Some(res)) => res,
                Ok(None) => break, // recv task ended after an engine error
                Err(_) => {
                    // nothing finalized yet: go drain stdin again (still honour the LEAVE deadline)
                    if leave_at.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(8)) {
                        println!(
                            "LEFT:{{\"after_ms\":{},\"forced\":true}}",
                            leave_at.unwrap().elapsed().as_millis()
                        );
                        let _ = io::stdout().flush();
                        break;
                    }
                    continue;
                }
            };
        match next {
            Ok(Some(message)) => {
                events_rx += 1;
                handle_message(message, &engine, mission.as_mut());
            }
            Ok(None) => {
                unknown_rx += 1;
                // After LEAVE a None is the engine's wind-down.
                if let Some(t) = leave_at {
                    println!("LEFT:{{\"after_ms\":{}}}", t.elapsed().as_millis());
                    let _ = io::stdout().flush();
                    break;
                }
            }
            Err(e) => {
                eprintln!("ENGINE_ERROR: {}", e);
                break;
            }
        }
        // The engine forces its own exit after its graceful-shutdown timeout
        // (5 s release); do not outlive it if no None ever arrives.
        if let Some(t) = leave_at {
            if t.elapsed() >= std::time::Duration::from_secs(8) {
                println!(
                    "LEFT:{{\"after_ms\":{},\"forced\":true}}",
                    t.elapsed().as_millis()
                );
                let _ = io::stdout().flush();
                break;
            }
        }
    }

    Ok(())
}

// ── Message handler ──────────────────────────────────────────────────────────

fn handle_message(message: Message, engine: &Engine, mission: Option<&mut StateSync>) {
    match message {
        Message::Event(event) => {
            if event.transaction_count() > 0 {
                // Safe metadata extraction — all direct field reads or proven-safe FFI
                let creator = format!("{}", event.creator());
                let created_at = event.created_at();
                let consensus_at = event.consensus_at();
                // Clock drift or event generation limits can cause created_at to be slightly after consensus_at
                // Use absolute difference to reflect total propagation/consensus delta
                let diff_ns = if consensus_at >= created_at {
                    consensus_at - created_at
                } else {
                    created_at - consensus_at
                };
                let latency_ms = diff_ns as f64 / 1_000_000.0;

                // Deterministic PRNG seed: use event.hash() — a 32-byte struct field,
                // zero FFI boundary risk. All peers that see this event compute the
                // same hash, so it serves the same role as whitened_signature() for
                // Python's consensus-driven attacker assignment.
                //
                // DO NOT call event.whitened_signature() — it invokes
                // tv_event_get_whitened_signature() which segfaults on gossip/heartbeat
                // events when famous witnesses haven't been computed yet.
                let hash_hex: String = event.hash().iter().map(|b| format!("{:02x}", b)).collect();

                // Collect transaction strings (null-strip before UTF-8 decode)
                let mut txns = Vec::new();
                let mut raw = Vec::new();
                for tx in event.transactions() {
                    let tx_bytes = tx.split(|&b| b == 0).next().unwrap_or(tx);
                    if let Ok(s) = from_utf8(tx_bytes) {
                        let s = s.trim();
                        if !s.is_empty() {
                            // Emit as a JSON string literal (properly escaped)
                            txns.push(format!("{:?}", s));
                            raw.push(s.to_string());
                        }
                    }
                }
                if let Some(sync) = mission {
                    sync.on_txs(raw);
                }

                println!(
                    "EVENT:{{\"creator\":\"{}\",\"created_at\":{},\"consensus_at\":{},\"latency_ms\":{:.3},\"whitened_sig\":\"{}\",\"txns\":[{}]}}",
                    creator,
                    created_at,
                    consensus_at,
                    latency_ms,
                    hash_hex,
                    txns.join(",")
                );
                let _ = io::stdout().flush();
            }
        }

        Message::SyncPoint(sync_point) => {
            // SyncPoint = consensus-level topology change
            // (node kicked or joined — ground truth for comm-loss detection)
            let epoch = sync_point.epoch_index();
            let just_synced = sync_point.is_just_synced();
            println!(
                "SYNC_POINT:{{\"info\":\"topology_change\",\"timestamp\":{},\"epoch\":{},\"just_synced\":{}}}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                epoch,
                just_synced
            );
            let _ = io::stdout().flush();
            if let Some(sync) = mission {
                sync.on_sync_point(engine, epoch, just_synced);
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Sends a string as a null-terminated transaction to the network.
// ── Fast lane: signed status over UDP, latest value wins ─────────────────────
//
// Status (position, battery, link) does not need agreed order, only the newest value, fast
// and authentic. It goes straight to every peer as one signed datagram, outside consensus.
// Frame: "TVF1" | seq u64 | sent_ms u64 | len u16 | payload | P-256 signature (64 bytes, r||s)
// signed with the node's Vertex key. Receivers check the signature against the sender's
// Vertex public key (looked up by source IP) and drop anything not newer than the last
// seq seen from that sender. Lines "FAST:<payload>" on stdin go out this way; accepted
// datagrams print as "FAST:<sender ip>|<payload>".
mod fast {
    use p256::ecdsa::signature::{Signer, Verifier};
    use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
    use p256::pkcs8::{DecodePrivateKey, DecodePublicKey};
    use std::collections::HashMap;
    use std::io::Write;
    use std::net::{IpAddr, SocketAddr, UdpSocket};

    const MAGIC: &[u8; 4] = b"TVF1";
    const HEADER: usize = 4 + 8 + 8 + 2;
    const SIG: usize = 64;

    pub struct Lane {
        sock: UdpSocket,
        key: SigningKey,
        peers: Vec<SocketAddr>,
        seq: u64,
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default()
    }

    pub fn signing_key(der: &[u8]) -> anyhow::Result<SigningKey> {
        if let Ok(k) = SigningKey::from_pkcs8_der(der) {
            return Ok(k);
        }
        Ok(SigningKey::from(p256::SecretKey::from_sec1_der(der)?))
    }

    pub fn verifying_key(der: &[u8]) -> anyhow::Result<VerifyingKey> {
        Ok(VerifyingKey::from_public_key_der(der)?)
    }

    /// Binds `bind_ip:port`, starts the receiver thread and returns the sending half.
    pub fn start(
        bind_ip: IpAddr,
        port: u16,
        key: SigningKey,
        peers: Vec<(IpAddr, VerifyingKey)>,
    ) -> anyhow::Result<Lane> {
        let sock = UdpSocket::bind(SocketAddr::new(bind_ip, port))?;
        let rx = sock.try_clone()?;
        let keys: HashMap<IpAddr, VerifyingKey> = peers.iter().cloned().collect();
        std::thread::spawn(move || receive(rx, keys));
        let peers = peers
            .iter()
            .map(|(ip, _)| SocketAddr::new(*ip, port))
            .collect();
        Ok(Lane {
            sock,
            key,
            peers,
            seq: 0,
        })
    }

    impl Lane {
        pub fn send(&mut self, payload: &[u8]) {
            let len = payload.len().min(1200);
            self.seq += 1;
            let mut buf = Vec::with_capacity(HEADER + len + SIG);
            buf.extend_from_slice(MAGIC);
            buf.extend_from_slice(&self.seq.to_be_bytes());
            buf.extend_from_slice(&now_ms().to_be_bytes());
            buf.extend_from_slice(&(len as u16).to_be_bytes());
            buf.extend_from_slice(&payload[..len]);
            let sig: Signature = self.key.sign(&buf);
            buf.extend_from_slice(&sig.to_bytes());
            for p in &self.peers {
                let _ = self.sock.send_to(&buf, p);
            }
        }
    }

    fn receive(sock: UdpSocket, keys: HashMap<IpAddr, VerifyingKey>) {
        let mut last: HashMap<IpAddr, u64> = HashMap::new();
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf) else {
                continue;
            };
            let m = &buf[..n];
            if n < HEADER + SIG || &m[..4] != MAGIC {
                continue;
            }
            let Some(vk) = keys.get(&from.ip()) else {
                continue;
            };
            let len = u16::from_be_bytes([m[20], m[21]]) as usize;
            if HEADER + len + SIG != n {
                continue;
            }
            let Ok(sig) = Signature::from_slice(&m[n - SIG..]) else {
                continue;
            };
            if vk.verify(&m[..n - SIG], &sig).is_err() {
                continue;
            }
            let seq = u64::from_be_bytes(m[4..12].try_into().unwrap());
            let seen = last.entry(from.ip()).or_insert(0);
            if seq <= *seen {
                continue; // stale or replayed: latest value wins
            }
            *seen = seq;
            let payload = String::from_utf8_lossy(&m[HEADER..HEADER + len]);
            let out = std::io::stdout();
            let mut o = out.lock();
            let _ = writeln!(o, "FAST:{}|{}", from.ip(), payload);
            let _ = o.flush();
        }
    }
}

fn send_transaction_cstr(engine: &Engine, s: &str) -> tashi_vertex::Result<()> {
    let mut transaction = Transaction::allocate(s.len() + 1);
    transaction[..s.len()].copy_from_slice(s.as_bytes());
    transaction[s.len()] = 0; // null-terminate (matches pingback.rs pattern)
    engine.send_transaction(transaction)
}
