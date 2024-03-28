use std::collections::VecDeque;
use std::num::NonZeroU32;
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
}

impl SampleSet {
    pub fn new(window_size: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            window_size,
        }
    }

    pub fn push(&mut self, sample: TpsData) {
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

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub fn generate_tps(count: usize, tps: u64) -> SampleSet {
        let mut samples = SampleSet::new(count);

        for _ in 0..count {
            let success_count = tps;
            let elapsed = Duration::from_secs(1);

            samples.push(TpsData {
                success_count,
                error_count: 0,
                elapsed,
            })
        }

        samples
    }

    #[test]
    fn test_generate_tps() {
        let res = generate_tps(10, 20_000);
        println!("{res:?}");
    }
}
