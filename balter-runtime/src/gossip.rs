use crate::error::RuntimeError;
use balter_core::ScenarioConfig;
use interchange::GossipStream;
use message::{Handshake, Message};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tracing::debug;
use url::Url;
use uuid::Uuid;

mod data;
mod error;
mod interchange;
pub(crate) mod message;
mod protocol;

pub(crate) use data::{GossipData, PeerInfo};
pub(crate) use error::GossipError;

pub(crate) async fn gossip_task(gossip: Gossip) -> Result<(), GossipError> {
    // TODO: This gossip interval rate is arbitrary at this point. It would be nice to either
    // ground this in a value that has some meaning, make it auto-adjusting, or even just have it
    // as a parameter.
    let mut interval = tokio::time::interval(Duration::from_millis(5000));

    loop {
        interval.tick().await;

        let peer = { gossip.data.lock()?.select_random_peer() };
        if let Some(peer) = peer {
            let mut stream = peer_stream(&peer).await?;
            gossip.request_sync(&mut stream, peer.addr).await?;
        } else {
            debug!("No peers to gossip with.");
        }
    }
}

type SpawnHook = fn(ScenarioConfig) -> Result<(), RuntimeError>;

#[derive(Clone)]
pub(crate) struct Gossip {
    server_id: Uuid,
    pub data: Arc<Mutex<GossipData>>,
    scenario_spawn_hook: SpawnHook,
}

impl Gossip {
    pub fn new(server_id: Uuid, port: u16, scenario_spawn_hook: SpawnHook) -> Self {
        Self {
            data: Arc::new(Mutex::new(GossipData::new(server_id, port))),
            server_id,
            scenario_spawn_hook,
        }
    }

    pub async fn receive_request(
        &self,
        stream: &mut impl GossipStream,
        peer_addr: SocketAddr,
    ) -> Result<(), GossipError> {
        let msg: Message<Handshake> = stream.recv().await?;
        match msg.inner() {
            Handshake::Sync => self.receive_sync_request(stream, peer_addr).await,
            Handshake::Help => self.receive_help_request(stream, peer_addr).await,
        }
    }
}

pub async fn peer_stream(peer: &PeerInfo) -> Result<impl GossipStream, GossipError> {
    let url = Url::parse(&format!("ws://{}/ws", peer.addr))?;
    let (stream, _) = connect_async(url).await?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::spawn_scenario;
    use axum::{extract::ws::WebSocketUpgrade, routing::get, Router};
    use tokio::sync::mpsc::{channel, Receiver, Sender};
    use tokio_tungstenite::connect_async;
    use url::Url;

    pub(crate) struct FakeStream {
        tx: Sender<Vec<u8>>,
        rx: Receiver<Vec<u8>>,
    }

    impl FakeStream {
        pub fn duplex() -> (Self, Self) {
            let (tx0, rx0) = channel(10);
            let (tx1, rx1) = channel(10);

            (
                FakeStream { tx: tx0, rx: rx1 },
                FakeStream { tx: tx1, rx: rx0 },
            )
        }
    }

    impl GossipStream for FakeStream {
        async fn recv_bytes(&mut self) -> Option<Result<Vec<u8>, GossipError>> {
            self.rx.recv().await.map(Ok)
        }

        async fn send_bytes(&mut self, bytes: Vec<u8>) -> Result<(), GossipError> {
            self.tx.send(bytes).await.unwrap();
            Ok(())
        }
    }

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn sync_test_with_machinery() {
        tokio::spawn(async {
            let app = Router::new().route(
                "/ws",
                get(|ws: WebSocketUpgrade| async {
                    ws.on_upgrade(move |mut socket| async move {
                        let gossip = Gossip::new(Uuid::new_v4(), 1111, spawn_scenario);
                        // TODO: This should come from Axum but requires some extra machinery I
                        // haven't done yet for the test.
                        let addr: SocketAddr = "0.0.0.0:7633".to_string().parse().unwrap();
                        gossip.receive_request(&mut socket, addr).await.unwrap();
                    })
                }),
            );

            let socket_addr: SocketAddr = "0.0.0.0:7633".to_string().parse().unwrap();
            let listener = tokio::net::TcpListener::bind(socket_addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let url = Url::parse("ws://0.0.0.0:7633/ws").unwrap();
        let (mut ws_stream, _) = connect_async(url).await.unwrap();
        let gossip = Gossip::new(Uuid::new_v4(), 1234, spawn_scenario);

        gossip
            .request_sync(
                &mut ws_stream,
                "0.0.0.0:7633".to_string().parse::<SocketAddr>().unwrap(),
            )
            .await
            .unwrap();

        let peer_count = gossip.data.lock().unwrap().peers.len();
        assert_eq!(peer_count, 2);
    }
}
