use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::time::Duration;

/// Minimal Run Statistics for a given Scenario
///
/// Provides a sliver of the statistics available from a given Scenario run. More stats will be
/// added over time.
///
/// TODO:
/// - Error Rate
/// - Measured TPS (Quantiles)
pub struct RunStatistics {
    pub concurrency: usize,
    pub tps: NonZeroU32,
    pub stable: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct TpsData {
    pub success_count: u64,
    pub error_count: u64,
    pub elapsed: Duration,
}

impl TpsData {
    #[allow(unused)]
    pub fn new() -> Self {
        Self {
            success_count: 0,
            error_count: 0,
            elapsed: Duration::new(0, 0),
        }
    }

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

#[derive(Debug)]
pub struct SampleSet<T> {
    samples: VecDeque<T>,
    window_size: usize,
}

impl<T> SampleSet<T> {
    pub fn new(window_size: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            window_size,
        }
    }

    pub fn push(&mut self, sample: T) {
        self.samples.push_back(sample);
        if self.samples.len() > self.window_size {
            self.samples.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn full(&self) -> bool {
        self.samples.len() == self.window_size
    }
}

impl SampleSet<f64> {
    pub fn mean(&self) -> Option<f64> {
        if self.samples.len() == self.window_size {
            let sum: f64 = self.samples.iter().sum();
            Some(sum / self.samples.len() as f64)
        } else {
            None
        }
    }

    #[allow(unused)]
    pub fn std(&self) -> Option<f64> {
        let mean = self.mean()?;

        let n = self.samples.len() as f64;
        let v = self.samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.);

        Some(v.sqrt())
    }
}

impl SampleSet<TpsData> {
    // TODO: Rather than return Option, we can have a method which returns a "Full" SampleSet with
    // non-optional return values here
    pub fn mean_err(&self) -> Option<f64> {
        if self.samples.len() == self.window_size {
            let sum: f64 = self.samples.iter().map(TpsData::error_rate).sum();
            Some(sum / self.samples.len() as f64)
        } else {
            None
        }
    }

    pub fn mean_tps(&self) -> Option<f64> {
        if self.samples.len() == self.window_size {
            let sum: f64 = self.samples.iter().map(TpsData::tps).sum();
            Some(sum / self.samples.len() as f64)
        } else {
            None
        }
    }
}
