//! Default Balter Distributed Runtime
//!
//! This Runtime handles running scenarios and distributing workloads to peers. Currently this
//! involves spinning up an API server and a gossip protocol task.
use crate::{
    error::RuntimeError,
    gossip::{gossip_task, peer_stream, Gossip},
    server::server_task,
    DistributedScenario,
};
use async_channel::{bounded, Receiver, Sender};
use balter_core::{RunStatistics, ScenarioConfig};
use clap::Parser;
use lazy_static::lazy_static;
#[doc(hidden)]
pub use linkme::distributed_slice;
use std::future::Future;
use std::pin::Pin;
use std::{collections::HashMap, net::SocketAddr};
#[allow(unused)]
use tracing::{debug, error, info, instrument, Instrument};

mod message;

pub use message::RuntimeMessage;

// TODO: This doesn't need to be a global, and can be threaded into each Scenario via task_local.
lazy_static! {
    /// Message queue for sending work to other peers
    pub static ref BALTER_OUT: (Sender<RuntimeMessage>, Receiver<RuntimeMessage>) =
        bounded(10);
}

/// An array created at link-time which stores the names of each scenario and their respective
/// function pointer.
#[doc(hidden)]
#[distributed_slice]
pub static BALTER_SCENARIOS: [(
    &'static str,
    fn() -> Pin<Box<dyn DistributedScenario<Output = RunStatistics>>>,
)];

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
/// ```ignore
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

impl Default for BalterRuntime {
    fn default() -> Self {
        Self::new()
    }
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

    #[instrument(name="balter", skip_all, fields(port=self.port))]
    pub async fn run(self) {
        let gossip = Gossip::new(uuid::Uuid::new_v4(), self.port, spawn_scenario);

        spawn_or_halt(server_task(self.port, gossip.clone())).await;
        spawn_or_halt(gossip_task(gossip.clone())).await;
        spawn_or_halt(helper_task(gossip.clone())).await;
    }
}

pub(crate) fn spawn_scenario(config: ScenarioConfig) -> Result<(), RuntimeError> {
    // TODO: We probably don't want to rebuild this every time.
    let scenarios: HashMap<_, _> = BALTER_SCENARIOS
        .iter()
        .enumerate()
        .map(|(idx, (name, _))| (*name, idx))
        .collect();

    let idx = scenarios
        .get(config.name.as_str())
        .ok_or(RuntimeError::NoScenario)?;
    info!("Running scenario {}.", &config.name);
    let scenario = BALTER_SCENARIOS[*idx];
    let fut = scenario.1().set_config(config);
    tokio::spawn(
        async move {
            fut.await;
        }
        .in_current_span(),
    );
    Ok(())
}

async fn helper_task(gossip: Gossip) -> Result<(), RuntimeError> {
    let (_, ref rx) = *BALTER_OUT;
    let rx = rx.clone();
    loop {
        if let Ok(msg) = rx.recv().await {
            match msg {
                RuntimeMessage::Help(config) => {
                    // TODO: The internal `data` probably shouldn't be exposed like this.
                    let peer = {
                        let mut data = gossip.data.lock()?;
                        data.set_state_busy();
                        data.select_free_peer()
                    };
                    if let Some(peer) = peer {
                        let mut stream = peer_stream(&peer).await?;
                        let res = gossip.request_help(&mut stream, peer.addr, config).await;
                        if let Err(error) = res {
                            error!("Error in gossip protocol: {error:?}");
                        }
                    } else {
                        error!("No Peers available to help.");
                        // TODO: Implement some form of retry/auto-scaling
                    }
                }
                RuntimeMessage::Finished => {
                    gossip.data.lock()?.set_state_free();
                }
            }
        } else {
            return Err(RuntimeError::ChannelClosed);
        }
    }
}

async fn spawn_or_halt<F, R, E>(fut: F)
where
    F: Future<Output = Result<R, E>> + Send + 'static,
{
    tokio::spawn(
        async move {
            let res = fut.await;
            if res.is_err() {
                error!("Failure in critical service. Shutting down.");
                std::process::exit(1);
            }
        }
        .in_current_span(),
    );
}
