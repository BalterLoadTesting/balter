//! Gossip Handling Module
//!
//! This module handles all of the logic related to the gossip protocol. Currently its in an
//! experimental state, with a number of latent issues and trivial inefficiencies. Frankly,
//! its going to need a bottom-up rethink.
//!
//! Right now the logic is bare-bones: connect to peer with most out-of-date timestamp, and send
//! all information back and forth and merge by taking the latest. There are a _lot_ of ways of
//! making this more efficient and less error-prone, but this works for now.
use crate::runtime::BALTER_OUT;
use axum::extract::ws::WebSocket;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use thiserror::Error;
use time::OffsetDateTime;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, instrument, trace};
use url::Url;

pub(crate) type SharedGossipData = Arc<RwLock<GossipData>>;

#[derive(Debug)]
pub(crate) struct GossipData {
    seed_list: HashSet<SocketAddr>,
    // TODO: Right now we use the SocketAddr as the ID for each peer. This is likely a bad idea and
    // we should use real IDs
    cluster_info: HashMap<SocketAddr, PeerInfo>,
    addr: Option<SocketAddr>,
    port: u16,
}

impl GossipData {
    pub fn new(seed_list: &[SocketAddr], port: u16) -> Self {
        let seed_list = HashSet::from_iter(seed_list.iter().copied());

        Self {
            seed_list,
            cluster_info: HashMap::new(),
            // TODO: Is there a way for the server to get its own SocketAddr here?
            addr: None,
            port,
        }
    }

    pub(crate) fn shared(self) -> SharedGossipData {
        Arc::new(RwLock::new(self))
    }

    /// Select peer for gossiping
    ///
    /// Currently this just operates on most-outdated-peer (ie. whichever peer has the oldest
    /// last_timestamp_utc value).
    fn select_gossip_peer(&self) -> Option<SocketAddr> {
        let (oldest_peer, _) = self
            .cluster_info
            .iter()
            // NOTE: This is safe because we only call this after we know our own addr
            .filter(|(&addr, _)| addr != self.addr.unwrap())
            // NOTE: This _should_ be okay, as an Unreachable peer will just be ignored
            // until it starts gossiping on its own (and hence update its own state).
            .filter(|(_, peer)| peer.state != PeerState::Unreachable)
            .min_by_key(|(_, peer)| peer.last_timestamp_utc)?;

        Some(*oldest_peer)
    }

    /// Select peer for giving work
    fn select_peer_for_work(&self) -> Option<SocketAddr> {
        let (free_peer, _) = self
            .cluster_info
            .iter()
            .filter(|(&addr, _)| {
                // NOTE: Must account for not having gossiped with any peer
                // TODO: Pretty gross there isn't symmetry here.
                if let Some(self_addr) = self.addr {
                    addr != self_addr
                } else {
                    true
                }
            })
            // NOTE: This _should_ be okay, as an Unreachable peer will just be ignored
            // until it starts gossiping on its own (and hence update its own state).
            .filter(|(_, peer)| peer.state == PeerState::Free)
            .min_by_key(|(_, peer)| peer.last_timestamp_utc)?;

        Some(*free_peer)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct PeerInfo {
    version: u64,
    last_timestamp_utc: OffsetDateTime,
    state: PeerState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PeerState {
    Free,
    Busy,
    Unreachable,
    Unknown,
}

#[derive(Error, Debug)]
pub(crate) enum GossipError {
    #[error("Poisoned lock error.")]
    PoisonError,

    #[error("Url parsing error: {0}")]
    UrlError(#[from] url::ParseError),

    #[error("Websocket error: {0}")]
    WsError(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("Error with serialization: {0}")]
    BincodeError(#[from] Box<bincode::ErrorKind>),

    #[error("Empty message received during gossip when one is expected")]
    NoMessage,

    #[error("Message received is not binary")]
    NonBinary,

    #[error("Message received has an invalid addr: Expected {0}, Actual {1}")]
    InvalidAddr(SocketAddr, SocketAddr),

    #[error("Axum websocket error: {0}")]
    AxumError(#[from] axum::Error),

    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
}

/// Background Gossip Task
///
/// A background worker to sync peer info by gossiping with known peers. Really a glorified
/// timer-based task to send out a request to a peer at pseudo-random and update info.
pub(crate) async fn gossip_task(shared_data: SharedGossipData) -> Result<(), GossipError> {
    // TODO: This gossip interval rate is arbitrary at this point. It would be nice to either
    // ground this in a value that has some meaning, make it auto-adjusting, or even just have it
    // as a parameter.
    let mut interval = tokio::time::interval(Duration::from_millis(5000));

    // TODO: Find a cleaner way to address sharing work
    tokio::spawn(work_gossip_task(shared_data.clone()));

    // 1. First we whittle down our "Seed" list
    // TODO: A lot of repeated logic in the two loops.
    loop {
        interval.tick().await;

        let peer = {
            let gossip_data = shared_data.read().map_err(|_| GossipError::PoisonError)?;
            if let Some(peer) = gossip_data.seed_list.iter().last() {
                *peer
            } else {
                // No more peers in our seed list; break out
                break;
            }
        };

        match gossip_with_peer(&shared_data, peer).await {
            Ok(_) => {
                debug!("Successfully gossiped with {peer}.");
                trace!("New State: {:?}", shared_data.read().unwrap());
                {
                    let mut data = shared_data.write().map_err(|_| GossipError::PoisonError)?;
                    data.seed_list.remove(&peer);
                }
            }
            Err(err) => {
                error!("Error gossiping: {err:?}");
            }
        }
    }

    debug!("Seed list complete.");

    // 2. Then we transition to the long-term loop
    loop {
        interval.tick().await;

        let peer: Option<SocketAddr> = shared_data
            .read()
            .map_err(|_| GossipError::PoisonError)?
            .select_gossip_peer();

        if let Some(peer) = peer {
            match gossip_with_peer(&shared_data, peer).await {
                Ok(_) => {
                    debug!("Successfully gossiped with {peer}.");
                    trace!("New State: {:?}", shared_data.read().unwrap());
                }
                Err(err) => {
                    error!("Error gossiping: {err:?}");
                }
            }
        } else {
            debug!("No peers available to gossip with");
        }
    }
}

async fn work_gossip_task(shared_data: SharedGossipData) -> Result<(), GossipError> {
    let (_, ref rx) = *BALTER_OUT;

    loop {
        match rx.recv().await {
            Ok(config) => {
                debug!("Requesting help from a peer for scenario {}", config.name);
                let addr = {
                    let mut data = shared_data.write().map_err(|_| GossipError::PoisonError)?;
                    if let Some(self_addr) = data.addr {
                        if let Some(val) = data.cluster_info.get_mut(&self_addr) {
                            // NOTE: Marking ourselves as Busy is currently a best-effort attempt
                            // at denying load. We need to be smarter about gossiping and work
                            // sharing to make this a better state.
                            val.state = PeerState::Busy;
                        }
                    }
                    data.select_peer_for_work()
                };

                let addr = if let Some(addr) = addr {
                    addr
                } else {
                    error!("No peers available for work.");
                    continue;
                };

                // TODO: Support https
                let url = Url::parse(&format!("http://{}/run", addr))?;
                let client = reqwest::Client::new();
                let res = client.post(url).json(&config).send().await?;

                // TODO: Handle peer errors (retry, find another peer, etc.)
                if !res.status().is_success() {
                    error!(
                        "Peer error: status_code={}, text={}",
                        res.status(),
                        res.text().await?
                    );
                }
            }
            Err(err) => {
                error!("Task for requesting help from peers has errored: {err:?}");
                break;
            }
        }
    }

    Ok(())
}

// TODO: This message can be far more efficient. Right now we just send everything we know, when
// realistically we can just send the version vector and figure out the exact data which needs to
// be synced.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct GossipMessage {
    addr: SocketAddr,
    cluster_info: HashMap<SocketAddr, PeerInfo>,
}

/// Gossip Initiation
///
/// The gossip protocol as used right now is very simplistic. It sends the version-vector and then
/// uses this to determine what data needs to be synced.
#[instrument(skip(shared_data))]
async fn gossip_with_peer(
    shared_data: &SharedGossipData,
    addr: SocketAddr,
) -> Result<(), GossipError> {
    let url = Url::parse(&format!("ws://{}/info-ws", addr))?;
    let (mut ws_stream, _) = connect_async(url).await?;
    trace!("Successfully negotiated stream");

    let data = {
        let cluster_info = shared_data
            .read()
            .map_err(|_| GossipError::PoisonError)?
            .cluster_info
            .clone();
        let message = GossipMessage { addr, cluster_info };
        bincode::serialize(&message)?
    };

    ws_stream.send(Message::Binary(data)).await?;

    let message = ws_stream.next().await.ok_or(GossipError::NoMessage)??;
    let message: GossipMessage = match message {
        Message::Binary(val) => bincode::deserialize(&val)?,
        _ => return Err(GossipError::NonBinary),
    };

    sync_cluster_info(shared_data, &message)?;

    Ok(())
}

/// Gossip Receiver
///
/// TODO: Unfortunately Axum provides a websocket handler which is tungstenite under-the-hood but
/// instead of re-exports its done such that the types are non-compatible. This is fairly frustrating,
/// and this function should probably be redone to be generic over Sink/Stream traits.
pub(crate) async fn receive_gossip(
    mut socket: WebSocket,
    shared_data: &SharedGossipData,
    peer_addr: SocketAddr,
) -> Result<(), GossipError> {
    let message = socket.recv().await.ok_or(GossipError::NoMessage)??;

    let message: GossipMessage = match message {
        axum::extract::ws::Message::Binary(val) => bincode::deserialize(&val)?,
        _ => return Err(GossipError::NonBinary),
    };

    let data = {
        sync_cluster_info(shared_data, &message)?;

        let message = GossipMessage {
            addr: peer_addr,
            cluster_info: shared_data
                .read()
                .map_err(|_| GossipError::PoisonError)?
                .cluster_info
                .clone(),
        };
        bincode::serialize(&message)?
    };

    socket
        .send(axum::extract::ws::Message::Binary(data))
        .await?;

    Ok(())
}

fn sync_cluster_info(
    shared_data: &SharedGossipData,
    message: &GossipMessage,
) -> Result<(), GossipError> {
    let shared_data = &mut shared_data.write().map_err(|_| GossipError::PoisonError)?;

    // 1. Ensure the addr in the message for our service matches what we have stored (or store
    //    one in case we don't have any stored)
    let self_addr = {
        let mut message_addr = message.addr;
        message_addr.set_port(shared_data.port);
        if let Some(addr) = shared_data.addr {
            if addr != message_addr {
                return Err(GossipError::InvalidAddr(addr, message_addr));
            }
            addr
        } else {
            shared_data.addr = Some(message_addr);
            message_addr
        }
    };

    // 2. Update our peer_info based on the largest version if applicable.
    for (addr, peer_info) in message.cluster_info.iter() {
        let local_peer_info = shared_data.cluster_info.get(addr);

        if let Some(local_peer_info) = local_peer_info {
            if local_peer_info.version < peer_info.version {
                shared_data.cluster_info.insert(*addr, peer_info.clone());
            }
        } else {
            shared_data.cluster_info.insert(*addr, peer_info.clone());
        }
    }

    // 3. Update our own information in our state.
    #[allow(clippy::map_entry)]
    if shared_data.cluster_info.contains_key(&self_addr) {
        // TODO: Unfortunately borrowchecker doesn't like when we use `if let` here, so it must be
        // broken up which is a bit gross.
        let info = shared_data.cluster_info.get_mut(&self_addr).unwrap();
        info.version += 1;
        info.last_timestamp_utc = OffsetDateTime::now_utc();
        // TODO: Is there any reason why we would need to update the state here?
    } else {
        // Must be our first time gossiping -- just putting in default values.
        shared_data.cluster_info.insert(
            self_addr,
            PeerInfo {
                version: 1,
                last_timestamp_utc: OffsetDateTime::now_utc(),
                state: PeerState::Free,
            },
        );
    }

    Ok(())
}
