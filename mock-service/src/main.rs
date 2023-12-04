use axum::{http::StatusCode, routing::get, Router};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use lazy_static::lazy_static;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

#[tokio::main]
async fn main() {
    tokio::task::spawn(async { tps_measure_task().await });

    let app = Router::new()
        .route("/api_10ms", get(get_10ms))
        .route("/api_max_tps", get(get_max_tps));

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

static TPS_MEASURE: AtomicU64 = AtomicU64::new(0);

async fn get_10ms() {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(10)).await;
}

async fn tps_measure_task() {
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let transactions = TPS_MEASURE.fetch_min(0, Ordering::Relaxed);
        println!("{transactions} TPS");
    }
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
