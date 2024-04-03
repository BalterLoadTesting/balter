use crate::{error::RuntimeError, gossip::Gossip, runtime::spawn_scenario};
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
use balter_core::ScenarioConfig;
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
    let state = ServerState { gossip };

    let app = Router::new()
        .route("/run", post(run))
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
    gossip: Gossip,
}

#[derive(Error, Debug)]
enum HandlerError {
    #[error("Channel send error (Balter runtime has likely fallen over): {0}")]
    Send(#[from] async_channel::SendError<ScenarioConfig>),

    #[error("Runtime error: {0}")]
    Runtime(#[from] RuntimeError),
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        use HandlerError::*;
        match self {
            Runtime(RuntimeError::NoScenario) => {
                (StatusCode::NOT_FOUND, "Scenario not found".to_string())
            }
            Send(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {err:?}"),
            ),
            Runtime(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Runtime error: {err:?}"),
            ),
        }
        .into_response()
    }
}

#[instrument(skip(_state))]
async fn run(
    State(_state): State<Arc<ServerState>>,
    Json(scenario): Json<ScenarioConfig>,
) -> Result<String, HandlerError> {
    let output = format!("Running scenario {}", &scenario.name);

    spawn_scenario(scenario).await?;

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
