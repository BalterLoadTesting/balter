use pdatastructs::tdigest::{TDigest, K1};
use std::time::Duration;

const TDIGEST_BACKLOG_SIZE: usize = 100;

#[derive(Debug)]
pub struct Measurements {
    pub tps: f64,
    pub error_rate: f64,
    pub elapsed: Duration,
    latency: TDigest<K1>,
}

impl Measurements {
    pub fn new(success: u64, error: u64, elapsed: Duration) -> Self {
        let tps = success as f64 / elapsed.as_secs_f64();
        let error_rate = error as f64 / (success + error) as f64;
        Self {
            tps,
            error_rate,
            elapsed,
            latency: default_tdigest(),
        }
    }

    pub fn populate_latencies(&mut self, dur: &[Duration]) {
        for latency in dur {
            self.latency.insert(latency.as_secs_f64());
        }
    }

    pub fn latency(&self, quantile: f64) -> Duration {
        let secs = self.latency.quantile(quantile);
        Duration::from_secs_f64(secs)
    }
}

fn default_tdigest() -> TDigest<K1> {
    // TODO: Double-check these values
    TDigest::new(K1::new(10.), TDIGEST_BACKLOG_SIZE)
}
