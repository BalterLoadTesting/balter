use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use tracing::error;
use uuid::Uuid;

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GossipData {
    pub peers: HashMap<Uuid, PeerInfoPartial>,
    pub server_id: Uuid,
    my_addr: MyAddress,
}

impl GossipData {
    pub fn new(server_id: Uuid, port: u16) -> Self {
        let peers = HashMap::new();
        Self {
            peers,
            server_id,
            my_addr: MyAddress::Unknown { port },
        }
    }

    pub fn hash(&self) -> u64 {
        let mut s = DefaultHasher::new();
        let peers: Vec<_> = self.peers.iter().collect();
        peers.hash(&mut s);
        s.finish()
    }

    pub fn merge(&mut self, mut other: GossipData) {
        self.peers.extend(other.peers.drain());
    }

    // NOTE: This ends up being an interesting problem: what _is_ the address of the
    // current server? Is there a way to reliably figure that out without pinging another
    // server? Essentially, we defer figuring this out until the first gossip interaction,
    // in which case we learn the value.
    pub fn learn_address(&mut self, mut addr: SocketAddr) {
        if let MyAddress::Unknown { port } = self.my_addr {
            // WebSocket port != server port
            addr.set_port(port);

            self.peers.insert(
                self.server_id,
                PeerInfoPartial {
                    state: PeerState::Free,

                    addr,
                    version: 1,
                },
            );

            self.my_addr = MyAddress::Known;
        }
    }

    pub fn select_random_peer(&self) -> Option<PeerInfo> {
        let mut rng = rand::thread_rng();
        self.peers
            .iter()
            .map(|(id, info)| PeerInfo::from_partial(*info, *id))
            .choose(&mut rng)
    }

    pub fn select_free_peer(&self) -> Option<PeerInfo> {
        let mut rng = rand::thread_rng();
        self.peers
            .iter()
            .filter_map(|(id, info)| {
                if matches!(info.state, PeerState::Free) {
                    Some(PeerInfo::from_partial(*info, *id))
                } else {
                    None
                }
            })
            .choose(&mut rng)
    }

    pub fn set_state_free(&mut self) {
        if let Some(info) = self.peers.get_mut(&self.server_id) {
            info.state = PeerState::Free;
        } else {
            error!("Unable to modify state.");
        }
    }

    pub fn set_state_busy(&mut self) {
        if let Some(info) = self.peers.get_mut(&self.server_id) {
            info.state = PeerState::Busy;
        } else {
            error!("Unable to modify state.");
        }
    }

    pub fn is_busy(&self) -> Option<bool> {
        match self.peers.get(&self.server_id) {
            Some(info) if info.state == PeerState::Busy => Some(true),
            Some(_info) => Some(false),
            None => None,
        }
    }
}

#[derive(Hash, PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub(crate) struct PeerInfo {
    pub server_id: Uuid,
    pub version: u64,
    pub addr: SocketAddr,
    pub state: PeerState,
}

impl PeerInfo {
    fn from_partial(partial: PeerInfoPartial, server_id: Uuid) -> PeerInfo {
        PeerInfo {
            server_id,
            version: partial.version,
            addr: partial.addr,
            state: partial.state,
        }
    }
}

#[derive(Hash, PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct PeerInfoPartial {
    version: u64,
    addr: SocketAddr,
    state: PeerState,
}

#[derive(Hash, PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) enum PeerState {
    Busy,
    Free,
    Unreachable,
}

// TODO: Naming is hard
#[derive(Hash, PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
enum MyAddress {
    Known,
    Unknown { port: u16 },
}
