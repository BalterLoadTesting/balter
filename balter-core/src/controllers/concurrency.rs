use crate::controllers::sample_set::SampleSet;
use std::collections::HashMap;
use std::num::NonZeroU32;
use tracing::{debug, error, info, trace, warn};

// TODO: Does it make more sense to have this as CPU count?
const STARTING_CONCURRENCY_COUNT: usize = 4;
const WINDOW_SIZE: usize = 20;

#[derive(Debug)]
pub(crate) struct ConcurrencyController {
    samples: SampleSet,
    prev_measurements: HashMap<usize, f64>,
    concurrency: usize,
    goal_tps: NonZeroU32,
    state: State,
}

impl ConcurrencyController {
    pub fn new(goal_tps: NonZeroU32) -> Self {
        Self {
            samples: SampleSet::new(WINDOW_SIZE),
            prev_measurements: HashMap::new(),
            concurrency: STARTING_CONCURRENCY_COUNT,
            goal_tps,
            state: State::Active,
        }
    }

    pub fn set_goal_tps(&mut self, goal_tps: NonZeroU32) {
        self.goal_tps = goal_tps;
        self.samples.clear();
        self.prev_measurements.clear();
        self.concurrency = STARTING_CONCURRENCY_COUNT;
        self.state = State::Active;
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub fn analyze(&mut self, sample: f64) -> Message {
        if sample == 0. {
            error!("No TPS sampled");
            return Message::None;
        }

        self.samples.push(sample);

        match self.analyze_inner() {
            Some(m @ Message::AlterConcurrency(val)) => {
                self.samples.clear();
                self.concurrency = val;
                info!("Adjusting concurrency to {}", self.concurrency);
                m
            }
            Some(m @ Message::TpsLimited(val)) => {
                warn!(
                    "TPS is limited to {}, at {} concurrency",
                    val, self.concurrency
                );
                m
            }
            Some(Message::None) | None => Message::None,
        }
    }

    fn analyze_inner(&mut self) -> Option<Message> {
        let mean_tps = self.samples.mean()?;
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
            self.state = State::Stable(0);
            None
        } else {
            self.prev_measurements
                .insert(self.concurrency, measurement.tps);

            let adjustment = goal_tps / measurement.tps;
            trace!(
                "Adjustment: {:.2} ({:.2} / {:.2})",
                adjustment,
                goal_tps,
                measurement.tps
            );

            let new_concurrency = (self.concurrency as f64 * adjustment) as usize;

            let new_concurrency_step = new_concurrency - self.concurrency;

            // TODO: Make this a proportion of the current concurrency so that it can scale faster
            // at higher levels.
            let new_concurrency = if new_concurrency_step > 50 {
                self.concurrency + 100
            } else {
                new_concurrency
            };

            if new_concurrency == 0 {
                error!("Error in the ConcurrencyController.");
                None
            } else if let Some(max_tps) = self.detect_underpowered() {
                Some(Message::TpsLimited(max_tps))
            } else {
                Some(Message::AlterConcurrency(new_concurrency))
            }
        }
    }

    fn detect_underpowered(&self) -> Option<NonZeroU32> {
        let mut data_points: Vec<Measurement> = self
            .prev_measurements
            .iter()
            .map(|(c, t)| Measurement {
                concurrency: *c,
                tps: *t,
            })
            .collect();

        data_points.sort_by_key(|f| f.concurrency);

        let slopes: Vec<_> = data_points
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

        if slopes.len() > 3 && slopes.iter().rev().take(3).all(|m| *m < 1.) {
            let last = data_points[data_points.len() - 1].tps;
            let max_tps = NonZeroU32::new(last as u32).unwrap();
            Some(max_tps)
        } else {
            None
        }
    }
}

#[derive(Debug)]
enum State {
    Active,
    Stable(usize),
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum Message {
    None,
    TpsLimited(NonZeroU32),
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
                Message::None => {}
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
                Message::None => {}
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
