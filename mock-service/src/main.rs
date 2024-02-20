use axum::{extract::Path, http::StatusCode, routing::get, Router};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use metrics_exporter_prometheus::PrometheusBuilder;
use lazy_static::lazy_static;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};
use metrics::{histogram, gauge, counter};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    PrometheusBuilder::new()
        .with_http_listener("0.0.0.0:3001".parse::<SocketAddr>().unwrap())
        .install()
        .unwrap();

    tokio::task::spawn(async { tps_measure_task().await });

    let app = Router::new()
        .route("/api_10ms", get(get_10ms))
        .route("/api_max_tps", get(get_max_tps))
        .route("/delay/ms/:delay_ms", get(delay))
        .route("/max/:max_tps", get(max))
        .route("/limited/:max_tps", get(limited));

    axum::Server::bind(&"0.0.0.0:3002".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn delay(Path(delay_ms): Path<u64>) {
    counter!("mock-server.tps").increment(1);
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

async fn max(Path(max_tps): Path<u32>) -> Result<(), StatusCode> {
    static LIMITER: OnceLock<DefaultDirectRateLimiter> = OnceLock::new();


    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(10)).await;

    let limiter = LIMITER
        .get_or_init(|| RateLimiter::direct(Quota::per_second(NonZeroU32::new(max_tps).unwrap())));
    match limiter.check() {
        Ok(_) => {
            counter!("mock-server.ok").increment(1);
            Ok(())
        }
        Err(_) => {
            counter!("mock-server.err").increment(1);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn limited(Path(max_tps): Path<u32>) -> Result<(), StatusCode> {
    static LIMITER: OnceLock<DefaultDirectRateLimiter> = OnceLock::new();

    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);

    let limiter = LIMITER
        .get_or_init(|| RateLimiter::direct(Quota::per_second(NonZeroU32::new(max_tps).unwrap())));

    tokio::time::sleep(Duration::from_millis(10)).await;
    limiter.until_ready().await;
    counter!("mock-server.tps").increment(1);
    Ok(())
}

/** Legacy; TODO: cleanup tests and move to new format **/

async fn get_10ms() {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(10)).await;
}

lazy_static! {
    static ref MAX_TPS: Arc<DefaultDirectRateLimiter> = Arc::new(RateLimiter::direct(
        Quota::per_second(NonZeroU32::new(500).unwrap())
    ));
}

async fn get_max_tps() -> Result<String, StatusCode> {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    match MAX_TPS.check() {
        Ok(_) => {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok("Ok".to_string())
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
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
