use std::str::{FromStr, from_utf8};

use anyhow::anyhow;
use clap::Parser;
use tashi_vertex::{
    Context, Engine, KeyPublic, KeySecret, Message, Options, Peers, Socket, Transaction,
};

#[derive(Debug, Clone)]
struct PeerArg {
    pub address: String,
    pub public: KeyPublic,
}

impl FromStr for PeerArg {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (public, address) = s
            .split_once('@')
            .ok_or_else(|| anyhow!("Invalid peer format, expected <public_key>@<address>"))?;

        let public = public.parse()?;
        let address = address.to_string();

        Ok(PeerArg { address, public })
    }
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(short = 'B')]
    pub bind: String,

    #[clap(short = 'K')]
    pub key: String,

    #[clap(short = 'P')]
    pub peers: Vec<PeerArg>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let key = args.key.parse::<KeySecret>()?;

    // initialize a set of peers for the network
    let mut peers = Peers::with_capacity(args.peers.len())?;

    for peer in &args.peers {
        peers.insert(&peer.address, &peer.public, Default::default())?;
    }

    // add ourself to the set of peers in the network
    peers.insert(&args.bind, &key.public(), Default::default())?;

    println!(" :: Configured network for {} peers", args.peers.len() + 1);

    // initialize a new Tashi Vertex (TV) context
    // manages async. operations and resources
    // allows for operations to complete
    let context = Context::new()?;

    println!(" :: Initialized runtime");

    // bind a new socket to listen for incoming connections in the network
    let socket = Socket::bind(&context, &args.bind)?.await?;

    println!(" :: Bound local socket");

    // configure execution options for the Tashi Vertex (TV) engine
    let mut options = Options::default();
    options.set_report_gossip_events(true);
    options.set_fallen_behind_kick_s(10);

    // start the engine
    // and begin participating in the network
    let engine = Engine::start(&context, socket, options, &key, peers, false)?;

    println!(" :: Started the consensus engine");

    // send an initial PING transaction
    send_transaction_cstr(&engine, "PING")?;

    // start waiting for messages
    while let Some(message) = engine.recv_message().await? {
        match message {
            Message::Event(event) => {
                if event.transaction_count() > 0 {
                    println!(" > Received EVENT");

                    // Print event metadata
                    println!("    - From: {}", event.creator());
                    println!("    - Created: {}", event.created_at());
                    println!("    - Consensus: {}", event.consensus_at());
                    println!("    - Transactions: {}", event.transaction_count());

                    // Print each transaction
                    for tx in event.transactions() {
                        // All transactions are strings
                        let tx_s = from_utf8(&tx)?;

                        println!("    - >> {}", tx_s);
                    }
                }
            }

            Message::SyncPoint(_) => {
                println!(" > Received SYNC POINT");
            }
        }
    }

    Ok(())
}

/// Sends a string as a null-terminated transaction to the network.
pub fn send_transaction_cstr(engine: &Engine, s: &str) -> tashi_vertex::Result<()> {
    let mut transaction = Transaction::allocate(s.len() + 1);

    transaction[..s.len()].copy_from_slice(s.as_bytes());
    transaction[s.len()] = 0; // null-terminate

    engine.send_transaction(transaction)
}
