use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Router,
};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use lazy_static::lazy_static;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

#[tokio::main]
async fn main() {
    tokio::task::spawn(async { tps_measure_task().await });

    let state = Arc::new(RwLock::new(None));

    let app = Router::new()
        .route("/api_10ms", get(get_10ms))
        .route("/api_max_tps", get(get_max_tps))
        .route("/delay/ms/:delay_ms", get(delay))
        .route("/max/:max_tps", get(max))
        .with_state(state);

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn delay(Path(delay_ms): Path<u64>) {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

type AppState = Arc<RwLock<Option<DefaultDirectRateLimiter>>>;

use std::ops::Deref;
async fn max(Path(max_tps): Path<u32>, State(state): State<AppState>) -> Result<(), StatusCode> {
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    match &state.read().unwrap().deref() {
        Some(limiter) => match limiter.check() {
            Ok(_) => Ok(()),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
        None => {
            let mut limiter = state.write().unwrap();
            *limiter = Some(RateLimiter::direct(Quota::per_second(
                NonZeroU32::new(max_tps).unwrap(),
            )));
            Ok(())
        }
    }
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
    }
}
