use pdatastructs::tdigest::{TDigest, K1};
use std::collections::VecDeque;
use std::time::Duration;

const TDIGEST_BACKLOG_SIZE: usize = 100;

#[derive(Debug)]
pub struct SampleSet {
    samples: VecDeque<SampleData>,
    latency: TDigest<K1>,
    window_size: usize,
    skip_first_n: Option<usize>,
    skip_window: usize,
    latency_skip_window: usize,
}

impl SampleSet {
    pub fn new(window_size: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            latency: default_tdigest(),
            window_size,
            skip_first_n: None,
            skip_window: 0,
            latency_skip_window: 0,
        }
    }

    pub fn skip_first_n(mut self, n_to_skip: usize) -> Self {
        self.skip_first_n = Some(n_to_skip);
        self.skip_window = n_to_skip;
        self.latency_skip_window = n_to_skip;
        self
    }

    pub fn push(&mut self, sample: SampleData) {
        if self.skip_window > 0 {
            self.skip_window -= 1;
            return;
        }

        self.samples.push_back(sample);
        if self.samples.len() > self.window_size {
            self.samples.pop_front();
        }
    }

    /// Separate Latency push method since the TDigest datastructure does not support merge, and is
    /// probabilistic in nature.
    pub fn push_latency(&mut self, latency: Duration) {
        if self.latency_skip_window > 0 {
            self.latency_skip_window -= 1;
            return;
        }

        self.latency.insert(latency.as_secs_f64());

        // TODO: The latency measurements have no windowing effect, and are strictly cumulative.
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.latency = default_tdigest();
        if let Some(skip_n) = self.skip_first_n {
            self.skip_window = skip_n;
            self.latency_skip_window = skip_n;
        }
    }

    pub fn full(&self) -> bool {
        self.samples.len() == self.window_size
    }

    // TODO: Rather than return Option, we can have a method which returns a "Full" SampleSet with
    // non-optional return values here
    pub fn mean_err(&self) -> Option<f64> {
        if self.samples.len() == self.window_size {
            let sum: f64 = self.samples.iter().map(SampleData::error_rate).sum();
            Some(sum / self.samples.len() as f64)
        } else {
            None
        }
    }

    pub fn mean_tps(&self) -> Option<f64> {
        if self.samples.len() == self.window_size {
            let sum: f64 = self.samples.iter().map(SampleData::tps).sum();
            Some(sum / self.samples.len() as f64)
        } else {
            None
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

#[derive(Debug, Clone)]
pub struct SampleData {
    pub success_count: u64,
    pub error_count: u64,
    pub elapsed: Duration,
}

impl SampleData {
    pub fn tps(&self) -> f64 {
        self.total() as f64 / self.elapsed.as_nanos() as f64 * 1e9
    }

    pub fn error_rate(&self) -> f64 {
        self.error_count as f64 / self.total() as f64
    }

    pub fn total(&self) -> u64 {
        self.success_count + self.error_count
    }
}
