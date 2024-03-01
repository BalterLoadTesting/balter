use serde::{Deserialize, Serialize};
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use uuid::Uuid;

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GossipData {
    pub peers: HashMap<Uuid, PeerInfo>,
    pub server_id: Uuid,
    my_addr: MyAddress,
}

impl GossipData {
    pub fn new(server_id: Uuid, port: u16) -> Self {
        let mut peers = HashMap::new();
        peers.insert(
            server_id,
            PeerInfo {
                state: PeerState::Free,

                // NOTE: This ends up being an interesting problem: what _is_ the address of the
                // current server? Is there a way to reliably figure that out without pinging another
                // server? Essentially, we defer figuring this out until the first gossip interaction,
                // in which case we learn the value.
                addr: None,
                version: 1,
            },
        );
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

    pub fn learn_address(&mut self, mut addr: SocketAddr) {
        if let MyAddress::Unknown { port } = self.my_addr {
            let info = self
                .peers
                .get_mut(&self.server_id)
                .expect("Own information is not present in State. This is a bug in Balter");

            // WebSocket port != server port
            addr.set_port(port);

            info.addr = Some(addr);
            self.my_addr = MyAddress::Known;
        }
    }

    pub fn select_random_peer(&self) -> Option<SocketAddr> {
        todo!()
    }
}

#[derive(Hash, PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PeerInfo {
    version: u64,
    addr: Option<SocketAddr>,
    state: PeerState,
}

#[derive(Hash, PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
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
