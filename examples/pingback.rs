use std::str::FromStr;

use anyhow::anyhow;
use clap::Parser;
use tashi_vertex::{Context, KeyPublic, KeySecret, Options, Peers, Socket};

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
    let socket = Socket::bind(&context, &args.bind).await?;

    println!(" :: Bound local socket");

    // configure execution options for the Tashi Vertex (TV) engine
    let mut options = Options::default();
    options.set_report_gossip_events(true);
    options.set_fallen_behind_kick_s(10);

    Ok(())
}
