use crate::{
    gossip::{receive_gossip, SharedGossipData},
    runtime::BALTER_IN,
    scenario::ScenarioConfig,
};
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
use std::{net::SocketAddr, sync::Arc};
use thiserror::Error;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{error, instrument, debug};

#[derive(Error, Debug)]
pub(crate) enum ServerError {
    #[error("Address Parsing Error")]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error("Address Parsing Error")]
    IoError(#[from] std::io::Error),
}

pub(crate) async fn server_task(port: u16, peers: SharedGossipData) -> Result<(), ServerError> {
    let (ref runtime_tx, _) = *BALTER_IN;
    let runtime_tx = runtime_tx.clone();
    let state = ServerState { peers, runtime_tx };

    let app = Router::new()
        .route("/run", post(run_scenario))
        .route("/info-ws", get(info_ws))
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
    peers: SharedGossipData,
    runtime_tx: Sender<ScenarioConfig>,
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

type HResult<T> = Result<T, HandlerError>;

#[instrument(skip(state))]
async fn run_scenario(
    State(state): State<Arc<ServerState>>,
    Json(scenario): Json<ScenarioConfig>,
) -> HResult<String> {
    let output = format!("Running scenario {}", &scenario.name);

    // TODO: Query runtime statistics to ensure this server can handle additional load.
    state.runtime_tx.send(scenario).await?;

    Ok(output)
}

async fn info_ws(
    State(state): State<Arc<ServerState>>,
    connection_info: ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, connection_info.0))
}

async fn handle_socket(socket: WebSocket, state: Arc<ServerState>, addr: SocketAddr) {
    if let Err(err) = receive_gossip(socket, &state.peers, addr).await {
        error!("Error in gossip: {err:?}");
    }
}
