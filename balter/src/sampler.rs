mod base_sampler;
mod outlier_detection;
mod task_atomics;
mod timer;

use crate::measurement::Measurement;
use std::future::Future;
use std::num::NonZeroU32;
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

const MIN_SAMPLES: usize = 5;
const MAX_RETRIES: usize = 4;

pub(crate) struct Sampler<T> {
    sampler: base_sampler::BaseSampler<T>,
    concurrency_history: Vec<(usize, f64)>,
    tps_limited: Option<(usize, NonZeroU32)>,
}

impl<T, F> Sampler<T>
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    pub async fn new(name: &str, scenario: T, tps_limit: NonZeroU32, concurrency: usize) -> Self {
        let mut sampler = base_sampler::BaseSampler::new(name, scenario, tps_limit).await;
        sampler.set_concurrency(concurrency);
        Self {
            sampler,
            concurrency_history: vec![],
            tps_limited: None,
        }
    }

    pub async fn sample(&mut self) -> (bool, Measurement) {
        let mut retries = 0;
        let mut prev = vec![];
        loop {
            let measurement = self.sampler.sample().await;
            prev.push(measurement.clone());

            if prev.len() < MIN_SAMPLES {
                continue;
            }

            let stats = calculate_stats(&prev);
            trace!("Stats: {stats:?}");

            // Check if the statistics have stabilized, if not we retry, and if
            // we have retried too many times we note with a warning.
            // TODO: Would be nice to have adaptable interval here.
            if stats.outlier_count > 0 || stats.std_percent() > 0.25 {
                prev.clear();
                retries += 1;

                if retries > MAX_RETRIES {
                    warn!("Significant statistical noise in measurements.");
                } else {
                    continue;
                }
            }

            if !self.check_underpowered() {
                self.adjust_concurrency(stats);
            }

            if self.at_goal(stats) {
                break (true, measurement);
            } else {
                break (false, measurement);
            }
        }
    }

    pub fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        self.sampler.set_tps_limit(tps_limit);
    }

    pub fn shutdown(self) -> SamplerStats {
        let concurrency = self.sampler.concurrency();
        let tps_limit = self.sampler.tps_limit();
        self.sampler.shutdown();

        SamplerStats {
            tps_limit,
            concurrency,
            tps_limited: self.tps_limited.is_some(),
        }
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.sampler.tps_limit()
    }

    fn check_underpowered(&mut self) -> bool {
        if self.tps_limited.is_some() {
            return true;
        }

        if self.concurrency_history.len() > 4
            && detect_zero_slope(&self.concurrency_history[self.concurrency_history.len() - 3..])
        {
            let (max_concurrency, max_tps) =
                self.concurrency_history[self.concurrency_history.len() - 3];

            let max_tps = max_tps * 0.9;
            let max_tps = NonZeroU32::new(max_tps.ceil().max(1.) as u32).unwrap();
            self.tps_limited = Some((max_concurrency, max_tps));
            self.sampler.set_tps_limit(max_tps);
            self.sampler.set_concurrency(max_concurrency);
            self.concurrency_history.clear();
            true
        } else {
            false
        }
    }

    fn at_goal(&self, stats: Stats) -> bool {
        let goal_tps = self.sampler.tps_limit().get() as f64;
        (stats.mean + stats.std) >= (goal_tps * 0.98)
    }

    fn adjust_concurrency(&mut self, stats: Stats) {
        self.concurrency_history
            .push((self.sampler.concurrency(), stats.mean));

        let tps_per_task = stats.mean / self.sampler.concurrency() as f64;
        let new_concurrency = (self.sampler.tps_limit().get() as f64 / tps_per_task).ceil();
        //Make sure not infinity in case of division by zero (when mean is 0)
        if new_concurrency.is_finite() {
            let new_concurrency = (new_concurrency as usize).max(self.sampler.concurrency()).max(1);
            self.sampler.set_concurrency(new_concurrency);
        }
    }
}

pub(crate) struct SamplerStats {
    pub tps_limit: NonZeroU32,
    pub concurrency: usize,
    pub tps_limited: bool,
}

#[derive(Debug, Copy, Clone)]
struct Stats {
    mean: f64,
    std: f64,
    #[allow(unused)]
    outlier_count: usize,
}

impl Stats {
    // Calculate Standard deviation as a percentage of the Mean
    fn std_percent(&self) -> f64 {
        1. - ((self.mean - self.std) / self.mean)
    }
}

fn calculate_stats(measurements: &[Measurement]) -> Stats {
    let tps: Vec<f64> = measurements.iter().map(|m| m.tps).collect();

    let mean = tps.iter().sum::<f64>() / tps.len() as f64;
    let var = tps.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / tps.len() as f64;
    let std = var.sqrt();

    let outlier_count = outlier_detection::num_outliers(&tps);

    Stats {
        mean,
        std,
        outlier_count,
    }
}

fn detect_zero_slope(values: &[(usize, f64)]) -> bool {
    let slopes: Vec<_> = values
        .windows(2)
        .map(|arr| {
            let (c0, t0) = arr[0];
            let (c1, t1) = arr[1];

            let slope = (t1 - t0) / (c1 - c0) as f64;

            if slope.is_nan() {
                error!("NaN Slope detected. Ignoring.");
                return 0.;
            }

            let b = t1 - slope * c0 as f64;
            trace!("({}, {:.2}), ({}, {:.2})", c0, t0, c1, t1,);
            trace!("y = {:.2}x + {:.2}", slope, b);

            slope
        })
        .collect();

    slopes.iter().all(|m| *m < 1.)
}
