use axum::{debug_handler, extract::Path, http::StatusCode, routing::get, Router};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use lazy_static::lazy_static;
#[allow(unused)]
use metrics::{counter, gauge, histogram};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::RwLock;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock as ARwLock,
    },
    time::Duration,
};
use tracing::debug;

pub async fn run(addr: SocketAddr) {
    let app = Router::new()
        .route("/delay/ms/:delay_ms", get(delay))
        .route(
            "/max/:max_tps/delay/ms/:delay_ms/scenario/:scenario_name",
            get(max),
        )
        .route(
            "/limited/:max_tps/delay/ms/:delay_ms/server/:server_id",
            get(limited),
        );

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[debug_handler]
pub async fn delay(Path(delay_ms): Path<u64>) {
    counter!("mock-server.tps").increment(1);
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

lazy_static! {
    static ref MAX_MAP: Arc<RwLock<HashMap<String, DefaultDirectRateLimiter>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

#[debug_handler]
pub async fn max(
    Path((max_tps, delay_ms, scenario_name)): Path<(u32, u64, String)>,
) -> Result<(), StatusCode> {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

    {
        if let Some(limiter) = MAX_MAP.read().unwrap().get(&scenario_name) {
            match limiter.check() {
                Ok(_) => {
                    debug!("MOCK SERVER ___ OK");
                    return Ok(());
                }
                Err(_) => {
                    debug!("MOCK SERVER ___ ERR");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

    debug!("MOCK SERVER ___ START");
    MAX_MAP
        .write()
        .unwrap()
        .insert(scenario_name, rate_limiter(max_tps));
    Ok(())
}

lazy_static! {
    static ref LIMITED_MAP: Arc<ARwLock<HashMap<String, Arc<DefaultDirectRateLimiter>>>> =
        Arc::new(ARwLock::new(HashMap::new()));
}

#[debug_handler]
pub async fn limited(
    Path((max_tps, delay_ms, server_id)): Path<(u32, u64, String)>,
) -> Result<(), StatusCode> {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

    let read = LIMITED_MAP.read().unwrap().get(&server_id).cloned();
    let limiter = if let Some(limiter) = read {
        limiter
    } else {
        let limiter = Arc::new(rate_limiter(max_tps));
        LIMITED_MAP
            .write()
            .unwrap()
            .insert(server_id, limiter.clone());
        limiter
    };

    limiter.until_ready().await;

    Ok(())
}

/** Utils **/

pub fn rate_limiter(tps: u32) -> DefaultDirectRateLimiter {
    RateLimiter::direct(Quota::per_second(NonZeroU32::new(tps).unwrap()))
}

/** TPS Printer **/

static TPS_MEASURE: AtomicU64 = AtomicU64::new(0);

pub async fn tps_measure_task() {
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let transactions = TPS_MEASURE.fetch_min(0, Ordering::Relaxed);
        println!("{transactions} TPS");
        //histogram!("mock-server.tps").record(transactions as f64);
    }
}
