use super::super::{message::Message, Gossip, GossipError, GossipStream};
use balter_core::ScenarioConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::error;

impl Gossip {
    #[allow(unused)]
    pub(crate) async fn request_help(
        &self,
        mut stream: &mut impl GossipStream,
        peer_addr: SocketAddr,
        config: ScenarioConfig,
    ) -> Result<(), GossipError> {
        stream.send(Message::help()).await?;

        stream.send(Message::run_config(config)).await?;

        let status: Message<Status> = stream.recv().await?;

        if matches!(status.inner(), Status::Busy) {
            Err(GossipError::PeerBusy)
        } else {
            Ok(())
        }
    }

    #[allow(unused)]
    pub(crate) async fn receive_help_request(
        &self,
        mut stream: &mut impl GossipStream,
        peer_addr: SocketAddr,
    ) -> Result<(), GossipError> {
        let msg: Message<RunConfig> = stream.recv().await?;

        // TODO: Be far more clever about whether this server can accept work
        let is_busy = self.data.lock()?.is_busy();

        match is_busy {
            Some(true) => {
                stream.send(Message::new(Status::Busy)).await?;
            }
            Some(false) => {
                stream.send(Message::new(Status::Accepted)).await?;
                // TODO: Handle error
                let _ = (self.scenario_spawn_hook)(msg.config());
            }
            None => {
                error!("Could not find own info.");
                stream.send(Message::new(Status::Busy)).await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunConfig {
    config: ScenarioConfig,
}

impl Message<RunConfig> {
    pub fn run_config(config: ScenarioConfig) -> Message<RunConfig> {
        Message {
            inner: RunConfig { config },
        }
    }

    pub fn config(self) -> ScenarioConfig {
        self.inner.config
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Status {
    Busy,
    Accepted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RuntimeError;
    use crate::gossip::tests::FakeStream;
    use crate::gossip::Gossip;
    use std::sync::atomic::{AtomicBool, Ordering};
    use uuid::Uuid;

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn help_test() {
        let gossip_0 = Gossip::new(Uuid::new_v4(), 1234, fake_spawn_scenario);
        let gossip_1 = Gossip::new(Uuid::new_v4(), 4321, fake_spawn_scenario);

        let (mut stream_0, mut stream_1) = FakeStream::duplex();

        // Let them sync first
        let (res0, res1) = tokio::join! {
            gossip_0.request_sync(&mut stream_0, "0.0.0.0:1111".parse().unwrap()),
            gossip_1.receive_request(&mut stream_1, "0.0.0.0:1111".parse().unwrap()),
        };

        assert!(res0.is_ok());
        assert!(res1.is_ok());

        let config = ScenarioConfig::new("test_config");
        let (res0, res1) = tokio::join! {
            gossip_0.request_help(&mut stream_0, "0.0.0.0:1111".parse().unwrap(), config),
            gossip_1.receive_request(&mut stream_1, "0.0.0.0:1111".parse().unwrap()),
        };

        assert!(res0.is_ok());
        assert!(res1.is_ok());

        assert!(SPAWNED.load(Ordering::Relaxed));
    }

    static SPAWNED: AtomicBool = AtomicBool::new(false);

    fn fake_spawn_scenario(_config: ScenarioConfig) -> Result<(), RuntimeError> {
        SPAWNED.store(true, Ordering::Relaxed);
        Ok(())
    }
}
