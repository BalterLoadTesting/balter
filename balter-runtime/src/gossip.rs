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

pub(crate) use data::GossipData;
pub(crate) use error::GossipError;
use interchange::GossipStream;
use message::{Handshake, Message};

pub(crate) async fn gossip_task(gossip: Gossip) -> Result<(), GossipError> {
    // TODO: This gossip interval rate is arbitrary at this point. It would be nice to either
    // ground this in a value that has some meaning, make it auto-adjusting, or even just have it
    // as a parameter.
    let mut interval = tokio::time::interval(Duration::from_millis(5000));

    loop {
        interval.tick().await;

        let peer_addr = gossip.data.lock()?.select_random_peer();
        if let Some(peer_addr) = peer_addr {
            let url = Url::parse(&format!("ws://{}/ws", peer_addr))?;
            let (stream, _) = connect_async(url).await?;
            gossip.request_sync(stream, peer_addr).await?;
        } else {
            debug!("No peers to gossip with.");
        }
    }
}

#[derive(Clone)]
pub(crate) struct Gossip {
    server_id: Uuid,
    data: Arc<Mutex<GossipData>>,
}

impl Gossip {
    pub fn new(server_id: Uuid, port: u16) -> Self {
        Self {
            data: Arc::new(Mutex::new(GossipData::new(server_id, port))),
            server_id,
        }
    }

    pub async fn receive_request(
        &self,
        mut stream: impl GossipStream,
        peer_addr: SocketAddr,
    ) -> Result<(), GossipError> {
        let msg: Message<Handshake> = stream.recv().await?;
        match msg.inner() {
            Handshake::Sync => self.receive_sync_request(stream, peer_addr).await,
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::ws::WebSocketUpgrade, routing::get, Router};
    use tokio_tungstenite::connect_async;
    use url::Url;

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn sync_test() {
        tokio::spawn(async {
            let app = Router::new().route(
                "/ws",
                get(|ws: WebSocketUpgrade| async {
                    ws.on_upgrade(move |socket| async {
                        let gossip = Gossip::new(Uuid::new_v4(), 1111);
                        // TODO: This should come from Axum but requires some extra machinery I
                        // haven't done yet for the test.
                        let addr: SocketAddr = "0.0.0.0:7633".to_string().parse().unwrap();
                        gossip.receive_request(socket, addr).await.unwrap();
                    })
                }),
            );

            let socket_addr: SocketAddr = "0.0.0.0:7633".to_string().parse().unwrap();
            let listener = tokio::net::TcpListener::bind(socket_addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let url = Url::parse("ws://0.0.0.0:7633/ws").unwrap();
        let (ws_stream, _) = connect_async(url).await.unwrap();
        let gossip = Gossip::new(Uuid::new_v4(), 1234);

        gossip
            .request_sync(
                ws_stream,
                "0.0.0.0:7633".to_string().parse::<SocketAddr>().unwrap(),
            )
            .await
            .unwrap();

        let peer_count = gossip.data.lock().unwrap().peers.len();
        assert_eq!(peer_count, 2);
    }
}
