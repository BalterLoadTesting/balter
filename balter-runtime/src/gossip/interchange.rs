use super::{message::Message, GossipError};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait GossipStream {
    async fn recv_bytes(&mut self) -> Option<Result<Vec<u8>, GossipError>>;
    async fn send_bytes(&mut self, bytes: Vec<u8>) -> Result<(), GossipError>;

    async fn try_recv<M: for<'a> Deserialize<'a>>(
        &mut self,
    ) -> Option<Result<Message<M>, GossipError>> {
        let bytes = self.recv_bytes().await?;

        Some((|| match bytes {
            Ok(bytes) => Ok(Message::from_bytes(&bytes)?),
            Err(err) => Err(err),
        })())
    }

    async fn recv<M: for<'a> Deserialize<'a>>(&mut self) -> Result<Message<M>, GossipError> {
        if let Some(res) = self.try_recv().await {
            res
        } else {
            Err(GossipError::NoData)
        }
    }

    async fn send<M: Serialize>(&mut self, message: Message<M>) -> Result<(), GossipError> {
        let bytes = message.to_bytes()?;
        self.send_bytes(bytes).await
    }
}

impl GossipStream for axum::extract::ws::WebSocket {
    async fn recv_bytes(&mut self) -> Option<Result<Vec<u8>, GossipError>> {
        let message = self.recv().await?;

        use axum::extract::ws::Message as AxumMessage;
        Some(match message {
            Ok(AxumMessage::Binary(bytes)) => Ok(bytes),
            Ok(_) => Err(GossipError::InvalidType),
            Err(err) => Err(GossipError::from(err)),
        })
    }

    async fn send_bytes(&mut self, bytes: Vec<u8>) -> Result<(), GossipError> {
        use axum::extract::ws::Message as AxumMessage;
        self.send(AxumMessage::Binary(bytes)).await?;
        Ok(())
    }
}

impl<T> GossipStream for tokio_tungstenite::WebSocketStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    async fn recv_bytes(&mut self) -> Option<Result<Vec<u8>, GossipError>> {
        let message = self.next().await?;
        use tungstenite::protocol::Message as TMessage;
        Some(match message {
            Ok(TMessage::Binary(bytes)) => Ok(bytes),
            Ok(_) => Err(GossipError::InvalidType),
            Err(err) => Err(GossipError::from(err)),
        })
    }

    async fn send_bytes(&mut self, bytes: Vec<u8>) -> Result<(), GossipError> {
        use tungstenite::protocol::Message as TMessage;
        <Self as SinkExt<TMessage>>::send(self, TMessage::Binary(bytes)).await?;
        Ok(())
    }
}
