use crate::tps_sampler::TpsData;
use std::collections::VecDeque;

#[derive(Debug)]
pub(crate) struct SampleSet<T> {
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
