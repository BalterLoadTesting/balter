use super::super::{message::Message, Gossip, GossipData, GossipError, GossipStream};
use balter_core::config::ScenarioConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

impl Gossip {
    #[allow(unused)]
    pub(crate) async fn request_help(
        &self,
        mut stream: impl GossipStream,
        peer_addr: SocketAddr,
        config: ScenarioConfig,
    ) -> Result<(), GossipError> {
        stream.send(Message::sync()).await?;
        Ok(())
    }

    #[allow(unused)]
    pub(crate) async fn receive_help_request(
        &self,
        mut stream: impl GossipStream,
        peer_addr: SocketAddr,
    ) -> Result<(), GossipError> {
        todo!()
    }
}
