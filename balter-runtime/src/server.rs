use crate::{gossip::Gossip, runtime::BALTER_IN};
use async_channel::Sender;
use axum::{
    extract::{
        connect_info::ConnectInfo,
        ws::{WebSocket, WebSocketUpgrade},
        Json, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use balter_core::config::ScenarioConfig;
use std::{net::SocketAddr, sync::Arc};
use thiserror::Error;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, instrument};

#[derive(Error, Debug)]
pub(crate) enum ServerError {
    #[error("Address Parsing Error")]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error("Address Parsing Error")]
    IoError(#[from] std::io::Error),
}

pub(crate) async fn server_task(port: u16, gossip: Gossip) -> Result<(), ServerError> {
    let (ref runtime_tx, _) = *BALTER_IN;
    let runtime_tx = runtime_tx.clone();
    let state = ServerState { runtime_tx, gossip };

    let app = Router::new()
        .route("/run", post(run_scenario))
        .route("/ws", get(ws))
        .with_state(Arc::new(state))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .into_make_service_with_connect_info::<SocketAddr>();

    let socket_addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(socket_addr).await?;

    debug!("Axum server starting up...");
    axum::serve(listener, app).await?;

    Ok(())
}

struct ServerState {
    runtime_tx: Sender<ScenarioConfig>,
    gossip: Gossip,
}

#[derive(Error, Debug)]
enum HandlerError {
    #[error("Channel send error (Balter runtime has likely fallen over): {0}")]
    SendError(#[from] async_channel::SendError<ScenarioConfig>),
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        use HandlerError::*;
        match self {
            SendError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {err:?}"),
            ),
        }
        .into_response()
    }
}

#[instrument(skip(state))]
async fn run_scenario(
    State(state): State<Arc<ServerState>>,
    Json(scenario): Json<ScenarioConfig>,
) -> Result<String, HandlerError> {
    let output = format!("Running scenario {}", &scenario.name);

    // TODO: Query runtime statistics to ensure this server can handle additional load.
    state.runtime_tx.send(scenario).await?;

    Ok(output)
}

async fn ws(
    State(state): State<Arc<ServerState>>,
    connection_info: ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws(socket, state, connection_info.0))
}

async fn handle_ws(socket: WebSocket, state: Arc<ServerState>, addr: SocketAddr) {
    let res = state.gossip.receive_request(socket, addr).await;
    if let Err(err) = res {
        error!("Error in gossip protocol: {err:?}");
    }
}
