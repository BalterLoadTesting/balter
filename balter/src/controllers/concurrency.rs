use balter_core::{SampleSet, TpsData};

use std::num::NonZeroU32;
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
            // NOTE: There is likely a smarter default here
            STARTING_CONCURRENCY_COUNT
        };

        self.set_goal_tps_with_concurrency(goal_tps, concurrency);
    }

    fn set_goal_tps_with_concurrency(&mut self, goal_tps: NonZeroU32, concurrency: usize) {
        self.goal_tps = goal_tps;
        self.prev_measurements.clear();
        self.concurrency = concurrency;
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub fn analyze(&mut self, samples: &SampleSet<TpsData>) -> AnalyzeResult {
        // TODO: Properly handle this error rather than panic
        let mean_tps = samples.mean_tps().expect("Invalid number of samples.");
        let measurement = Measurement {
            concurrency: self.concurrency,
            tps: mean_tps,
        };

        let goal_tps: f64 = Into::<u32>::into(self.goal_tps).into();

        debug!(
            "Goal TPS: {}, Measured TPS: {} at {} concurrency",
            goal_tps, measurement.tps, self.concurrency
        );

        let error = (goal_tps - measurement.tps) / goal_tps;
        if error < 0.05 {
            // NOTE: We don't really care about the negative case, since we're relying on the
            // RateLimiter to handle that situation.
            AnalyzeResult::Stable
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
                AnalyzeResult::AlterConcurrency(self.concurrency)
            } else if let Some((max_tps, concurrency)) = self.detect_underpowered() {
                AnalyzeResult::TpsLimited(max_tps, concurrency)
            } else {
                AnalyzeResult::AlterConcurrency(new_concurrency)
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
pub(crate) enum AnalyzeResult {
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
    use rand_distr::{Distribution, Normal};

    #[tracing_test::traced_test]
    #[test]
    fn scales_up() {
        let mut controller = ConcurrencyController::new(NonZeroU32::new(200).unwrap());

        let mut normal = Normal::new(9. * STARTING_CONCURRENCY_COUNT as f64, 2.).unwrap();
        for _ in 0..50 {
            let v: f64 = normal.sample(&mut rand::thread_rng());
            match controller.analyze(v) {
                Message::None | Message::Stable => {}
                Message::AlterConcurrency(new_val) => {
                    normal = Normal::new(9. * new_val as f64, 2.).unwrap();
                }
                Message::TpsLimited(_) => {
                    panic!("ConcurrencyController reports TpsLimited incorrectly");
                }
            }

            if matches!(controller.state, State::Stable(_)) {
                break;
            }
        }

        assert!(controller.concurrency > 20);
    }

    #[tracing_test::traced_test]
    #[test]
    fn limits() {
        let mut controller = ConcurrencyController::new(NonZeroU32::new(400).unwrap());

        let mut normal = Normal::new(9. * STARTING_CONCURRENCY_COUNT as f64, 2.).unwrap();
        let mut limited = false;
        for _ in 0..100 {
            let v: f64 = normal.sample(&mut rand::thread_rng());
            match controller.analyze(v) {
                Message::None | Message::Stable => {}
                Message::AlterConcurrency(new_val) => {
                    if new_val > 22 {
                        normal = Normal::new(9. * 22., 2.).unwrap();
                    } else {
                        normal = Normal::new(9. * new_val as f64, 2.).unwrap();
                    }
                }
                Message::TpsLimited(tps) => {
                    assert!(u32::from(tps) < 220);
                    assert!(u32::from(tps) > 170);
                    limited = true;
                    break;
                }
            }

            if matches!(controller.state, State::Stable(_)) {
                break;
            }
        }

        assert!(limited);
        assert!(controller.concurrency > 20);
    }
}
