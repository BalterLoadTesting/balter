use crate::data::SampleSet;
use crate::sampler::base_sampler::BaseSampler;
use std::future::Future;
use std::num::NonZeroU32;
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

const MAX_CHANGE: usize = 100;

// NOTE: Somewhat tricky to explain, but essentially our optimal concurrency search algorithm only
// increases concurrency. This means if we set concurrency to an "optimal" value, the search algo
// will immediately start increasing it (leading to a negative feedback loop with increased
// contention). This adjustment is a bit of a hack, where we always allow the concurrency to grow
// so that the algorithm stabilizes.
// TODO: Rewrite the concurrency search algorithm (see above NOTE)
const CONCURRENCY_SET_ADJUSTMENT: f64 = 0.75;

pub(crate) struct ConcurrencyAdjustedSampler<T> {
    sampler: BaseSampler<T>,
    measurements: Vec<(usize, f64)>,
    starting_concurrency: usize,
    tps_limited: bool,
}

impl<T, F> ConcurrencyAdjustedSampler<T>
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    pub async fn new(name: &str, scenario: T, tps_limit: NonZeroU32, concurrency: usize) -> Self {
        let mut sampler = BaseSampler::new(name, scenario, tps_limit).await;
        sampler.set_concurrency(concurrency);
        Self {
            sampler,
            measurements: vec![],
            starting_concurrency: concurrency,
            tps_limited: false,
        }
    }

    pub async fn sample(&mut self) -> (bool, SampleSet) {
        let samples = self.sampler.sample().await;

        let measured_tps = samples.mean_tps();
        let goal_tps = self.sampler.tps_limit().get() as f64;

        let error = (goal_tps - measured_tps) / goal_tps;
        if error < 0.05 {
            // NOTE: We don't really care about the negative case, since we're relying on the
            // RateLimiter to handle that situation.
            (true, samples)
        } else {
            let new_concurrency = self.adjust_concurrency(measured_tps);
            self.sampler.set_concurrency(new_concurrency);
            (false, samples)
        }
    }

    pub fn set_tps_limit(&mut self, limit: NonZeroU32) {
        if limit > self.sampler.tps_limit() && self.tps_limited {
            return;
        }

        self.sampler.set_tps_limit(limit);
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.sampler.tps_limit()
    }

    pub fn shutdown(self) -> SamplerStats {
        let concurrency = self.concurrency();
        let tps_limit = self.sampler.tps_limit();
        self.sampler.shutdown();

        SamplerStats {
            tps_limit,
            concurrency,
            tps_limited: self.tps_limited,
        }
    }

    pub fn concurrency(&self) -> usize {
        self.sampler.concurrency()
    }

    fn adjust_concurrency(&mut self, measured_tps: f64) -> usize {
        let concurrency = self.sampler.concurrency();
        let goal_tps = self.sampler.tps_limit().get() as f64;

        self.measurements.push((concurrency, measured_tps));

        let adjustment = goal_tps / measured_tps;

        let new_concurrency = (concurrency as f64 * adjustment).ceil() as usize;

        let new_concurrency_step = new_concurrency - concurrency;

        // TODO: Make this a proportion of the current concurrency so that it can scale faster
        // at higher levels.
        let new_concurrency = if new_concurrency_step > MAX_CHANGE {
            concurrency + MAX_CHANGE
        } else {
            new_concurrency
        };

        if new_concurrency == 0 {
            error!("Error in the ConcurrencyController.");
            self.starting_concurrency
        } else if let Some((max_tps, concurrency)) = self.detect_underpowered() {
            self.tps_limited = true;
            self.sampler.set_tps_limit(max_tps);
            (concurrency as f64 * CONCURRENCY_SET_ADJUSTMENT) as usize
        } else {
            new_concurrency
        }
    }

    fn detect_underpowered(&self) -> Option<(NonZeroU32, usize)> {
        let slopes: Vec<_> = self
            .measurements
            .windows(2)
            .map(|arr| {
                let (c0, t0) = arr[0];
                let (c1, t1) = arr[1];

                let slope = (t1 - t0) / (c1 - c0) as f64;

                // NOTE: The controller can get stuck at a given concurrency, and results in NaN.
                // This is an edge-case of when the controller is limited.
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

        if slopes.len() > 2 && slopes.iter().rev().take(2).all(|m| *m < 1.) {
            // Grab the minimum concurrency for the max TPS.
            let (concurrency, tps) = self.measurements[self.measurements.len() - 3];
            let max_tps = NonZeroU32::new(tps as u32).unwrap();
            Some((max_tps, concurrency))
        } else {
            None
        }
    }
}

pub(crate) struct SamplerStats {
    pub tps_limit: NonZeroU32,
    pub concurrency: usize,
    pub tps_limited: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_scenario;
    use rand_distr::{Distribution, SkewNormal};
    use std::time::Duration;

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_simple() {
        let mut sampler = ConcurrencyAdjustedSampler::new(
            "",
            mock_scenario!(Duration::from_millis(1), Duration::from_micros(10)),
            NonZeroU32::new(2_000).unwrap(),
            4,
        )
        .await;

        let _samples = sampler.sample().await;
        let _samples = sampler.sample().await;
        assert_eq!(sampler.concurrency(), 5);
    }
}
