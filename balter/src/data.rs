use pdatastructs::tdigest::{TDigest, K1};
use std::collections::VecDeque;
use std::time::Duration;

const TDIGEST_BACKLOG_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub struct SampleSet {
    samples: VecDeque<SampleData>,
    latency: TDigest<K1>,
}

impl SampleSet {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            latency: default_tdigest(),
        }
    }

    pub fn push(&mut self, sample: SampleData) {
        self.samples.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Separate Latency push method since the TDigest datastructure does not support merge, and is
    /// probabilistic in nature.
    pub fn push_latencies(&mut self, mut latencies: Vec<Duration>) {
        for latency in latencies.drain(..) {
            self.latency.insert(latency.as_secs_f64());
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.latency = default_tdigest();
    }

    pub fn mean_err(&self) -> f64 {
        let sum: f64 = self.samples.iter().map(SampleData::error_rate).sum();
        sum / self.samples.len() as f64
    }

    pub fn mean_tps(&self) -> f64 {
        let sum: f64 = self.samples.iter().map(SampleData::tps).sum();
        sum / self.samples.len() as f64
    }

    #[allow(unused)]
    pub fn var_tps(&self) -> f64 {
        let mean = self.mean_tps();
        self.samples
            .iter()
            .map(|x| (x.tps() - mean).powi(2))
            .sum::<f64>()
            / self.samples.len() as f64
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

#[derive(Debug, Clone)]
pub struct SampleData {
    pub success: u64,
    pub error: u64,
    pub elapsed: Duration,
}

impl SampleData {
    pub fn tps(&self) -> f64 {
        self.total() as f64 / self.elapsed.as_nanos() as f64 * 1e9
    }

    pub fn error_rate(&self) -> f64 {
        self.error as f64 / self.total() as f64
    }

    pub fn total(&self) -> u64 {
        self.success + self.error
    }
}
