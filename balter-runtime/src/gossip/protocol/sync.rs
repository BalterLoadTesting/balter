use super::super::{message::Message, Gossip, GossipData, GossipError, GossipStream};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

impl Gossip {
    pub(crate) async fn request_sync(
        &self,
        mut stream: impl GossipStream,
        peer_addr: SocketAddr,
    ) -> Result<(), GossipError> {
        stream.send(Message::sync()).await?;

        let hash = { self.data.lock()?.hash() };
        stream
            .send(Message::syn(&self.server_id, hash, peer_addr))
            .await?;

        let msg: Message<Ack> = stream.recv().await?;
        {
            self.data.lock()?.learn_address(msg.addr());
        }
        if msg.hash() == hash {
            stream.send(Message::fin()).await?;
            return Ok(());
        }

        // TODO: We really only want to send a _diff_ based on the hash...
        let msg = {
            let data = self.data.lock()?;
            Message::data(&data)?
        };
        stream.send(msg).await?;

        let msg: Message<Data> = stream.recv().await?;

        let peer_data = msg.read()?;

        {
            let mut data = self.data.lock()?;
            data.merge(peer_data);
        }

        stream.send(Message::fin()).await?;

        Ok(())
    }

    pub(crate) async fn receive_sync_request(
        &self,
        mut stream: impl GossipStream,
        peer_addr: SocketAddr,
    ) -> Result<(), GossipError> {
        let msg: Message<Syn> = stream.recv().await?;
        let hash = {
            let mut data = self.data.lock()?;
            data.learn_address(msg.addr());
            data.hash()
        };

        // NOTE: We want to send an Ack so the other serve can learn our ID
        stream
            .send(Message::ack(&self.server_id, hash, peer_addr))
            .await?;

        if msg.hash() == hash {
            let _: Message<Fin> = stream.recv().await?;
            return Ok(());
        }

        let msg: Message<Data> = stream.recv().await?;
        let peer_data = msg.read()?;

        let msg = {
            let mut data = self.data.lock()?;
            data.merge(peer_data);
            Message::data(&data)?
        };

        stream.send(msg).await?;

        let _: Message<Fin> = stream.recv().await?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Syn {
    server_id: Uuid,
    hash: u64,
    addr: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ack {
    server_id: Uuid,
    hash: u64,
    addr: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Data {
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fin {}

impl Message<Syn> {
    pub fn syn(server_id: &Uuid, hash: u64, addr: SocketAddr) -> Message<Syn> {
        Message {
            inner: Syn {
                server_id: server_id.to_owned(),
                hash,
                addr,
            },
        }
    }

    pub fn hash(&self) -> u64 {
        self.inner.hash
    }

    pub fn addr(&self) -> SocketAddr {
        self.inner.addr
    }
}

impl Message<Ack> {
    pub fn ack(server_id: &Uuid, hash: u64, addr: SocketAddr) -> Message<Ack> {
        Message {
            inner: Ack {
                server_id: server_id.to_owned(),
                hash,
                addr,
            },
        }
    }

    pub fn hash(&self) -> u64 {
        self.inner.hash
    }

    pub fn addr(&self) -> SocketAddr {
        self.inner.addr
    }
}

impl Message<Data> {
    pub fn data(data: &GossipData) -> Result<Message<Data>, GossipError> {
        // Serialize here so we aren't cloning the data out of the Mutex
        Ok(Message {
            inner: Data {
                bytes: bincode::serialize(data)?,
            },
        })
    }

    pub fn read(&self) -> Result<GossipData, GossipError> {
        Ok(bincode::deserialize(&self.inner.bytes)?)
    }
}

impl Message<Fin> {
    pub fn fin() -> Message<Fin> {
        Message { inner: Fin {} }
    }
}
