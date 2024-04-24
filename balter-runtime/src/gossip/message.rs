use super::GossipError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Message<M> {
    pub(crate) inner: M,
}

impl<M> Message<M> {
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

impl<M: Serialize> Message<M> {
    pub fn to_bytes(&self) -> Result<Vec<u8>, GossipError> {
        Ok(bincode::serialize(self)?)
    }
}

impl<M: for<'a> Deserialize<'a>> Message<M> {
    pub fn from_bytes(bytes: &[u8]) -> Result<Message<M>, GossipError> {
        Ok(bincode::deserialize(bytes)?)
    }
}

impl<M> Message<M> {
    pub(crate) fn inner(&self) -> &M {
        &self.inner
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Handshake {
    Sync,
    Help,
}

impl Message<Handshake> {
    pub fn sync() -> Self {
        Message {
            inner: Handshake::Sync,
        }
    }

    #[allow(unused)]
    pub fn help() -> Self {
        Message {
            inner: Handshake::Help,
        }
    }
}
