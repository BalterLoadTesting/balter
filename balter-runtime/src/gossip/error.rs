use std::sync::PoisonError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GossipError {
    #[error("Invalid WebSocket message type")]
    InvalidType,

    #[error("Error in Axum: {0}")]
    Axum(#[from] axum::Error),

    #[error("Error in Tungstenite: {0}")]
    Tungstenite(#[from] tungstenite::Error),

    #[error("Error deserializing with Bincode: {0}")]
    Bincode(#[from] Box<bincode::ErrorKind>),

    #[error("Stream ended too early")]
    NoData,

    #[error("GossipData Mutex is poisoned")]
    PoisonData,

    #[error("Error in parsing URL. This is a bug in Balter. {0}")]
    UrlError(#[from] url::ParseError),

    #[error("Peer to share work with is busy. Retries not implemented yet.")]
    PeerBusy,
}

impl<T> From<PoisonError<T>> for GossipError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonData
    }
}
