use axum::{debug_handler, extract::Path, http::StatusCode, routing::get, Router};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use lazy_static::lazy_static;
#[allow(unused)]
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
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

#[tokio::main]
async fn main() {
    PrometheusBuilder::new()
        .with_http_listener("0.0.0.0:3001".parse::<SocketAddr>().unwrap())
        .install()
        .unwrap();

    tokio::task::spawn(async { tps_measure_task().await });

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

    axum::Server::bind(&"0.0.0.0:3002".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[debug_handler]
async fn delay(Path(delay_ms): Path<u64>) {
    counter!("mock-server.tps").increment(1);
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

lazy_static! {
    static ref MAX_MAP: Arc<RwLock<HashMap<String, DefaultDirectRateLimiter>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

#[debug_handler]
async fn max(
    Path((max_tps, delay_ms, scenario_name)): Path<(u32, u64, String)>,
) -> Result<(), StatusCode> {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

    if let Some(limiter) = MAX_MAP.read().unwrap().get(&scenario_name) {
        match limiter.check() {
            Ok(_) => Ok(()),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        MAX_MAP
            .write()
            .unwrap()
            .insert(scenario_name, rate_limiter(max_tps));
        Ok(())
    }
}

lazy_static! {
    static ref LIMITED_MAP: Arc<ARwLock<HashMap<String, Arc<DefaultDirectRateLimiter>>>> =
        Arc::new(ARwLock::new(HashMap::new()));
}

#[debug_handler]
async fn limited(
    Path((max_tps, delay_ms, server_id)): Path<(u32, u64, String)>,
) -> Result<(), StatusCode> {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

    let limiter = if let Some(limiter) = LIMITED_MAP.read().unwrap().get(&server_id) {
        limiter.clone()
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

/** TPS Printer **/

static TPS_MEASURE: AtomicU64 = AtomicU64::new(0);

async fn tps_measure_task() {
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let transactions = TPS_MEASURE.fetch_min(0, Ordering::Relaxed);
        println!("{transactions} TPS");
        //histogram!("mock-server.tps").record(transactions as f64);
    }
}

/** Utils **/

fn rate_limiter(tps: u32) -> DefaultDirectRateLimiter {
    RateLimiter::direct(Quota::per_second(NonZeroU32::new(tps).unwrap()))
}
