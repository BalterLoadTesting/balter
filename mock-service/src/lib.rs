use axum::{
    debug_handler,
    extract::{Json, Path},
    http::StatusCode,
    routing::get,
    Router,
};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use lazy_static::lazy_static;
#[allow(unused)]
use metrics::{counter, gauge, histogram};
use rand_distr::{Distribution, SkewNormal};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::RwLock;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock as ARwLock,
    },
    time::Duration,
};
#[allow(unused)]
use tracing::{debug, error, instrument};

pub mod prelude {
    pub use super::{Config, LatencyConfig, LatencyKind, TpsConfig, TpsKind};
}

pub async fn run(addr: SocketAddr) {
    tokio::spawn(tps_updater_task());
    let app = Router::new()
        .route("/", get(mock_route))
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

/* New Handler */

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub scenario_name: String,
    pub tps: Option<TpsConfig>,
    pub latency: Option<LatencyConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TpsConfig {
    pub tps: NonZeroU32,
    pub kind: TpsKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TpsKind {
    CutOff,
    Error,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyConfig {
    pub latency: Duration,
    pub kind: LatencyKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LatencyKind {
    Delay,
    Linear(NonZeroU32),
    Noise(Duration, f64),
    //Exponential(NonZeroU32),
    //Cutoff(NonZeroU32),
}

lazy_static! {
    static ref SCENARIO_MAP: Arc<RwLock<HashMap<String, Arc<ScenarioState>>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

struct ScenarioState {
    tps_rate_limiter: Option<DefaultDirectRateLimiter>,
    tps_tracker: AtomicU64,
    avg_tps: AtomicU64,
    seen: AtomicBool,
}

#[instrument]
pub async fn mock_route(Json(config): Json<Config>) -> Result<(), StatusCode> {
    if config.tps.is_none() && config.latency.is_none() {
        error!("Garbage configuration for mock server");
        return Err(StatusCode::BAD_REQUEST);
    }

    let state = {
        let state = SCENARIO_MAP
            .read()
            .unwrap()
            .get(&config.scenario_name)
            .cloned();
        if let Some(state) = state {
            state
        } else {
            let state = Arc::new(ScenarioState {
                tps_rate_limiter: config
                    .tps
                    .as_ref()
                    .map(|tps_conf| rate_limiter(tps_conf.tps.get())),
                tps_tracker: AtomicU64::new(0),
                avg_tps: AtomicU64::new(0),
                seen: AtomicBool::new(false),
            });
            {
                let mut writer = SCENARIO_MAP.write().unwrap();
                writer.insert(config.scenario_name.clone(), state.clone());
            }
            state
        }
    };

    if let Some(tps_conf) = &config.tps {
        match tps_conf.kind {
            TpsKind::CutOff => {
                state.tps_rate_limiter.as_ref().unwrap().until_ready().await;
            }
            TpsKind::Error => {
                if state.tps_rate_limiter.as_ref().unwrap().check().is_err() {
                    counter!(format!("mock-server.{}.error", &config.scenario_name)).increment(1);
                    return Err(StatusCode::TOO_MANY_REQUESTS);
                }
            }
        }
    }

    if let Some(latency_conf) = &config.latency {
        match latency_conf.kind {
            LatencyKind::Delay => {
                tokio::time::sleep(latency_conf.latency).await;
                histogram!(format!("mock-server.{}.latency", &config.scenario_name))
                    .record(latency_conf.latency.as_secs_f64());
            }
            LatencyKind::Noise(std, shape) => {
                let skew_normal =
                    SkewNormal::new(latency_conf.latency.as_secs_f64(), std.as_secs_f64(), shape)
                        .unwrap();
                let v: f64 = skew_normal.sample(&mut rand::thread_rng());

                tokio::time::sleep(Duration::from_secs_f64(v)).await;
                histogram!(format!("mock-server.{}.latency", &config.scenario_name))
                    .record(latency_conf.latency.as_secs_f64());
            }
            LatencyKind::Linear(latency_tps) => {
                let avg_tps = state.avg_tps.load(Ordering::Relaxed);

                if avg_tps != 0 {
                    let ratio = avg_tps as f64 / latency_tps.get() as f64;
                    let wait = ratio * latency_conf.latency.as_secs_f64();
                    let wait = Duration::from_secs_f64(wait);
                    tokio::time::sleep(wait).await;
                    histogram!(format!("mock-server.{}.latency", &config.scenario_name))
                        .record(wait.as_secs_f64());
                }
            }
        }
    }

    counter!(format!("mock-server.{}.success", &config.scenario_name)).increment(1);
    state.tps_tracker.fetch_add(1, Ordering::Relaxed);

    Ok(())
}

/// Background task to keep track of average TPS. NOTE: This will have a bit of delay
/// as compared to the "instantaneous" TPS. Unfortunately this can really mess with the
/// LatencyController -- as the latency delay is calculated via this avg_tps measurement,
/// there is a possibility of delaying for too long even though TPS has dropped. The refresh
/// window needs to be small enough to adjust rapidly, but large enough to not run into issues
/// with noisy data (currently 10ms by guessing).
async fn tps_updater_task() {
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let scenario_map = SCENARIO_MAP.read().unwrap();
        for (name, state) in scenario_map.iter() {
            let transactions = state.tps_tracker.fetch_min(0, Ordering::Relaxed);
            if !state.seen.fetch_or(true, Ordering::Relaxed) {
                // We want to skip the first time we measure, since we don't know when the scenario
                // started logging transactions (ie. if it was in the middle of our sleep then our
                // TPS measurement will be skewed)
                continue;
            }

            let tps = transactions as f64 / 0.01;
            gauge!(format!("mock-server.{}.avg_tps", name)).set(tps);
            state.avg_tps.store(tps as u64, Ordering::Relaxed);
        }
    }
}

/* Old Handlers */

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
    counter!("mock-server.tps").increment(1);
    TPS_MEASURE.fetch_add(1, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

    {
        if let Some(limiter) = MAX_MAP.read().unwrap().get(&scenario_name) {
            match limiter.check() {
                Ok(_) => {
                    return Ok(());
                }
                Err(_) => {
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

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
    counter!("mock-server.tps").increment(1);
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

/* Utils */

pub fn rate_limiter(tps: u32) -> DefaultDirectRateLimiter {
    RateLimiter::direct(Quota::per_second(NonZeroU32::new(tps).unwrap()))
}

/* TPS Printer */

static TPS_MEASURE: AtomicU64 = AtomicU64::new(0);

pub async fn tps_measure_task() {
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let transactions = TPS_MEASURE.fetch_min(0, Ordering::Relaxed);
        println!("{transactions} TPS");
        //histogram!("mock-server.tps").record(transactions as f64);
    }
}
