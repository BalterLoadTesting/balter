use std::sync::PoisonError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RuntimeError {
    #[error("No scenario found")]
    NoScenario,

    #[error("Helper task channel closed unexpectedly.")]
    ChannelClosed,

    #[error("Mutex is poisoned.")]
    PoisonData,

    #[error("Gossip protocol had an error: {0}")]
    GossipProtocol(#[from] crate::gossip::GossipError),
}

impl<T> From<PoisonError<T>> for RuntimeError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonData
    }
}
