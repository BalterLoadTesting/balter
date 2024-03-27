use std::collections::VecDeque;
use std::time::Duration;

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
pub struct SampleSet {
    samples: VecDeque<TpsData>,
    window_size: usize,
    skip_first_n: Option<usize>,
    skip_window: usize,
}

impl SampleSet {
    pub fn new(window_size: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            window_size,
            skip_first_n: None,
            skip_window: 0,
        }
    }

    pub fn skip_first_n(mut self, n_to_skip: usize) -> Self {
        self.skip_first_n = Some(n_to_skip);
        self.skip_window = n_to_skip;
        self
    }

    pub fn push(&mut self, sample: TpsData) {
        if self.skip_window > 0 {
            self.skip_window -= 1;
            return;
        }

        self.samples.push_back(sample);
        if self.samples.len() > self.window_size {
            self.samples.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        if let Some(skip_n) = self.skip_first_n {
            self.skip_window = skip_n;
        }
    }

    pub fn full(&self) -> bool {
        self.samples.len() == self.window_size
    }

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
