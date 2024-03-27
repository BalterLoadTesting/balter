use super::super::{message::Message, Gossip, GossipError, GossipStream};
use balter_core::ScenarioConfig;

use std::net::SocketAddr;

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
