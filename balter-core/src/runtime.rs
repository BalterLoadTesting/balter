//! Default Balter Distributed Runtime
//!
//! This Runtime handles running scenarios and distributing workloads to peers. Currently this
//! involves spinning up an API server and a gossip protocol task.
use crate::{
    gossip::{gossip_task, GossipData},
    scenario::{ScenarioConfig, DistributedScenario},
    server::server_task,
};
use async_channel::{bounded, Receiver, Sender};
use clap::Parser;
use lazy_static::lazy_static;
#[doc(hidden)]
pub use linkme::distributed_slice;
use std::{collections::HashMap, net::SocketAddr};
use tracing::{debug, error, info, instrument, Instrument};

lazy_static! {
    /// Message queue for ingesting work (either from peers or user)
    pub(crate) static ref BALTER_IN: (Sender<ScenarioConfig>, Receiver<ScenarioConfig>) =
        bounded(10);

    /// Message queue for sending work to other peers
    pub(crate) static ref BALTER_OUT: (Sender<ScenarioConfig>, Receiver<ScenarioConfig>) =
        bounded(10);
}

/// An array created at link-time which stores the names of each scenario and their respective
/// function pointer.
#[doc(hidden)]
#[distributed_slice]
pub static BALTER_SCENARIOS: [(&'static str, fn() -> Box<dyn DistributedScenario<Output=()>>)];

const DEFAULT_PORT: u16 = 7621;

#[derive(Parser, Debug)]
#[command(version = "0.1")]
struct BalterCli {
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    port: u16,

    #[arg(short('n'), long)]
    peers: Vec<SocketAddr>,
}

/// Default Balter distributed runtime. (requires `rt` feature)
///
/// Creates a background API server to handle HTTP requests (for kicking off scenarios) as well
/// as a background task for handling the gossip protocol.
///
/// # Example
///
/// ```no_run
/// use balter::prelude::*;
///
/// #[tokio::main]
/// async fn main() {
///     BalterRuntime::new()
///         .with_args()
///         .run()
///         .await;
/// }
/// ```
pub struct BalterRuntime {
    port: u16,
    peers: Vec<SocketAddr>,
}

impl BalterRuntime {
    pub fn new() -> Self {
        BalterRuntime {
            port: DEFAULT_PORT,
            peers: vec![],
        }
    }

    /// Use the default CLI arguments for Balter.
    ///
    /// `-p`, `--port` to set a custom port number (default `7621`)
    ///
    /// `-n`, `--peers` to provide addresses to peer servers to enable gossiping.
    ///
    /// # Example
    /// ```ignore
    /// $ ./my_load_test -p 2742
    /// $ ./my_load_test -n 127.0.0.1:7621 -n 127.0.0.2:7621
    /// ```
    pub fn with_args(mut self) -> Self {
        let args = BalterCli::parse();
        self.port = args.port;
        self.peers = args.peers;
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn peers(mut self, peers: &[SocketAddr]) -> Self {
        self.peers = peers.to_vec();
        self
    }

    pub async fn run(self) {
        let scenarios: HashMap<_, _> = BALTER_SCENARIOS
            .iter()
            .enumerate()
            .map(|(idx, (name, _))| (*name, idx))
            .collect();
        debug!("Scenario mapping: {:?}", &scenarios);
        run(scenarios, &self).await.unwrap();
    }
}

impl Default for BalterRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[instrument(name="balter", skip_all, fields(port=balter.port))]
async fn run(scenarios: HashMap<&'static str, usize>, balter: &BalterRuntime) -> Result<(), ()> {
    let port = balter.port;
    let gossip_data = GossipData::new(&balter.peers, balter.port).shared();

    let gd = gossip_data.clone();
    tokio::spawn(async move { server_task(port, gd).await }.in_current_span());

    let gd = gossip_data.clone();
    tokio::spawn(async move { gossip_task(gd).await }.in_current_span());

    let (_, ref rx) = *BALTER_IN;
    let rx = rx.clone();
    loop {
        if let Ok(config) = rx.recv().await {
            if let Some(idx) = scenarios.get(config.name.as_str()) {
                info!("Running scenario {}.", &config.name);
                let scenario = BALTER_SCENARIOS[*idx];
                let fut = scenario.1().set_config(config);
                tokio::spawn(
                    async move {
                        fut.await;
                    }
                    .in_current_span(),
                );
            } else {
                error!("No scenario with name \"{}\" exists.", &config.name);
            }
        }
    }
}
