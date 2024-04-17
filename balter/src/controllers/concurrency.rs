use balter_core::SampleSet;
use std::num::NonZeroU32;
#[allow(unused)]
use tracing::{debug, error, trace};

// TODO: Does it make more sense to have this as CPU count?
const STARTING_CONCURRENCY_COUNT: usize = 4;
const MAX_CHANGE: usize = 100;

#[derive(Debug)]
pub(crate) struct ConcurrencyController {
    prev_measurements: Vec<Measurement>,
    concurrency: usize,
    goal_tps: NonZeroU32,
}

impl ConcurrencyController {
    pub fn new(goal_tps: NonZeroU32) -> Self {
        Self {
            prev_measurements: Vec::new(),
            concurrency: STARTING_CONCURRENCY_COUNT,
            goal_tps,
        }
    }

    pub fn set_goal_tps(&mut self, goal_tps: NonZeroU32) {
        let concurrency = if goal_tps > self.goal_tps {
            self.concurrency
        } else {
            // TODO: Better numerical conversions
            let ratio = goal_tps.get() as f64 / self.goal_tps.get() as f64;
            let new_concurrency =
                (ratio * self.concurrency as f64 + 1.).max(STARTING_CONCURRENCY_COUNT as f64);
            new_concurrency as usize
        };

        self.set_goal_tps_with_concurrency(goal_tps, concurrency);
    }

    fn set_goal_tps_with_concurrency(&mut self, goal_tps: NonZeroU32, concurrency: usize) {
        self.goal_tps = goal_tps;
        self.prev_measurements.clear();
        self.concurrency = concurrency;
    }

    #[allow(unused)]
    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub fn analyze(&mut self, samples: &SampleSet) -> CCOutcome {
        // TODO: Properly handle this error rather than panic
        let mean_tps = samples.mean_tps();
        let measurement = Measurement {
            concurrency: self.concurrency,
            tps: mean_tps,
        };

        let goal_tps: f64 = Into::<u32>::into(self.goal_tps).into();

        trace!(
            "Goal TPS: {}, Measured TPS: {} at {} concurrency",
            goal_tps,
            measurement.tps,
            self.concurrency
        );

        let error = (goal_tps - measurement.tps) / goal_tps;
        if error < 0.05 {
            // NOTE: We don't really care about the negative case, since we're relying on the
            // RateLimiter to handle that situation.
            CCOutcome::Stable
        } else {
            self.prev_measurements.push(measurement);

            let adjustment = goal_tps / measurement.tps;
            trace!(
                "Adjustment: {:.2} ({:.2} / {:.2})",
                adjustment,
                goal_tps,
                measurement.tps
            );

            let new_concurrency = (self.concurrency as f64 * adjustment).ceil() as usize;

            let new_concurrency_step = new_concurrency - self.concurrency;

            // TODO: Make this a proportion of the current concurrency so that it can scale faster
            // at higher levels.
            let new_concurrency = if new_concurrency_step > MAX_CHANGE {
                self.concurrency + MAX_CHANGE
            } else {
                new_concurrency
            };

            if new_concurrency == 0 {
                error!("Error in the ConcurrencyController.");
                self.concurrency = STARTING_CONCURRENCY_COUNT;
                CCOutcome::AlterConcurrency(self.concurrency)
            } else if let Some((max_tps, concurrency)) = self.detect_underpowered() {
                self.concurrency = concurrency;
                CCOutcome::TpsLimited(max_tps, concurrency)
            } else {
                self.concurrency = new_concurrency;
                CCOutcome::AlterConcurrency(new_concurrency)
            }
        }
    }

    fn detect_underpowered(&self) -> Option<(NonZeroU32, usize)> {
        let slopes: Vec<_> = self
            .prev_measurements
            .windows(2)
            .map(|arr| {
                let m1 = arr[0];
                let m2 = arr[1];

                let slope = (m2.tps - m1.tps) / (m2.concurrency - m1.concurrency) as f64;

                // NOTE: The controller can get stuck at a given concurrency, and results in NaN.
                // This is an edge-case of when the controller is limited.
                if slope.is_nan() {
                    return 0.;
                }

                let b = m2.tps - slope * m1.concurrency as f64;
                trace!(
                    "({}, {:.2}), ({}, {:.2})",
                    m1.concurrency,
                    m1.tps,
                    m2.concurrency,
                    m2.tps
                );
                trace!("y = {:.2}x + {:.2}", slope, b);

                slope
            })
            .collect();

        if slopes.len() > 2 && slopes.iter().rev().take(2).all(|m| *m < 1.) {
            // Grab the minimum concurrency for the max TPS.
            let x = self.prev_measurements[self.prev_measurements.len() - 3];
            let max_tps = NonZeroU32::new(x.tps as u32).unwrap();
            let concurrency = x.concurrency;
            Some((max_tps, concurrency))
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum CCOutcome {
    Stable,
    TpsLimited(NonZeroU32, usize),
    AlterConcurrency(usize),
}

#[derive(Debug, Copy, Clone)]
struct Measurement {
    pub concurrency: usize,
    pub tps: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use balter_core::SampleData;
    use std::num::NonZeroU32;
    use std::time::Duration;

    pub fn generate_tps(count: usize, tps: u64) -> SampleSet {
        let mut samples = SampleSet::new(count);

        for _ in 0..count {
            let success_count = tps;
            let elapsed = Duration::from_secs(1);

            samples.push(SampleData {
                success_count,
                error_count: 0,
                elapsed,
            })
        }

        samples
    }

    #[tracing_test::traced_test]
    #[test]
    fn scales_up() {
        let mut c = ConcurrencyController::new(NonZeroU32::new(200).unwrap());
        let starting_concurrency = c.concurrency();
        let samples = generate_tps(10, 100);

        match c.analyze(&samples) {
            CCOutcome::AlterConcurrency(concurrency) => {
                if concurrency <= starting_concurrency {
                    panic!("ConcurrencyController did not increase concurrency");
                }
            }
            _ => {
                panic!("Incorrect CCResult");
            }
        }
    }

    #[tracing_test::traced_test]
    #[test]
    fn limits() {
        let mut c = ConcurrencyController::new(NonZeroU32::new(200).unwrap());
        let samples = generate_tps(10, 100);

        let mut tps_limit = None;
        for _ in 0..10 {
            match c.analyze(&samples) {
                CCOutcome::AlterConcurrency(_) => {}
                CCOutcome::TpsLimited(max_tps, _) => {
                    tps_limit = Some(max_tps);
                    break;
                }
                CCOutcome::Stable => {
                    panic!("Incorrect CCResult");
                }
            }
        }

        assert_eq!(tps_limit, Some(NonZeroU32::new(100).unwrap()));
    }
}
