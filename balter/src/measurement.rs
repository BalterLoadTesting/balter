use pdatastructs::tdigest::{TDigest, K1};
use std::fmt;
use std::time::Duration;
use tracing::error;

const TDIGEST_BACKLOG_SIZE: usize = 100;

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Measurement {
    pub tps: f64,
    pub error_rate: f64,
    pub elapsed: Duration,
    latency: TDigest<K1>,
}

impl Measurement {
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

        // TODO: Unfortunately TDigest sometimes returns NaN which we need to filter for.
        let secs = if secs.is_finite() {
            secs
        } else {
            error!("NaN Latency Calculation. This is a known bug in Balter.");
            0.
        };

        Duration::from_secs_f64(secs)
    }
}

impl fmt::Display for Measurement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TPS={:.2}, ErrorRate={:.2}, p50={:?}, p90={:?}, p99={:?}",
            self.tps,
            self.error_rate,
            self.latency(0.5),
            self.latency(0.90),
            self.latency(0.99),
        )
    }
}

fn default_tdigest() -> TDigest<K1> {
    // TODO: Double-check these values
    TDigest::new(K1::new(10.), TDIGEST_BACKLOG_SIZE)
}
