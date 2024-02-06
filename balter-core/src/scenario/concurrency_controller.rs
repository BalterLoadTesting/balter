use std::collections::VecDeque;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

// TODO: Calculate experimentally
const SAMPLE_WINDOW: usize = 20;

#[derive(Debug)]
pub(crate) struct ConcurrencyController {
    samples: VecDeque<f64>,
    previous_measured_values: Vec<ConcurrencyMeasurements>,
    concurrent_count: u64,
    goal_tps: f64,
    state: ConcurrencyControllerState,
    underpowered_counter: Option<(u8, u64)>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum ConcurrencyControllerState {
    Adaptive,
    Stable,
    Underpowered(f64),
    Reset,
}

impl ConcurrencyController {
    pub(crate) fn new(goal_tps: f64) -> Self {
        // TODO: Make an error
        assert!(goal_tps > 0.);

        Self {
            samples: VecDeque::new(),
            previous_measured_values: Vec::new(),
            concurrent_count: 1,
            goal_tps,
            state: ConcurrencyControllerState::Adaptive,
            underpowered_counter: None,
        }
    }

    pub(crate) fn push(&mut self, sample: f64) {
        self.samples.push_back(sample);

        if self.samples.len() > SAMPLE_WINDOW {
            let _ = self.samples.pop_front();
            self.analyze();
        }
    }

    pub(crate) fn concurrent_count(&self) -> u64 {
        self.concurrent_count
    }

    pub(crate) fn is_underpowered(&self) -> Option<f64> {
        if let ConcurrencyControllerState::Underpowered(measurement) = self.state {
            Some(measurement)
        } else {
            None
        }
    }

    pub(crate) fn is_stable(&self) -> bool {
        self.state == ConcurrencyControllerState::Stable
    }

    pub(crate) fn set_goal_tps(&mut self, goal_tps: f64) -> bool {
        if (goal_tps - self.goal_tps).abs() > f64::EPSILON {
            self.goal_tps = goal_tps;
            self.reset();
            true
        } else {
            false
        }
    }

    pub(crate) fn reset(&mut self) {
        self.state = ConcurrencyControllerState::Adaptive;
        self.previous_measured_values.clear();
        self.samples.clear();
    }

    #[instrument(skip(self), fields(cc=self.concurrent_count))]
    fn analyze(&mut self) {
        let mean = mean(&self.samples);

        let error = (self.goal_tps - mean) / self.goal_tps;
        if error.abs() < 0.05 {
            if self.state != ConcurrencyControllerState::Stable {
                self.state = ConcurrencyControllerState::Stable;
                info!(
                    "Concurrency controller is stable. Goal: {:.2}, acheiving: {:.2} at concurrency {}",
                    self.goal_tps,
                    mean,
                    self.concurrent_count
                );
            }
        } else if error.is_sign_positive() {
            let std = std(&self.samples);

            if std > mean {
                debug!("Too much noise in data to adapt. mean={mean}, std={std}. Resetting.");
                self.reset();
            } else {
                let cur_measurements = ConcurrencyMeasurements {
                    concurrent_count: self.concurrent_count,
                    mean,
                    std,
                };

                /*
                 * Transition table:
                 * (TODO: There is likely a simpler way of doing this)
                | state          | cond   | res          | equivalent     |
                |----------------|--------|--------------|----------------|
                | x[]            |        | x+[x]        | x[x-]          |
                |                |        |              |                |
                | x[x-]          | x > x- | x+[x,x-]     | x[x-,x--]      |
                |                | x < x- | x-[x,x-]     | x[x+,x]        |
                |                |        |              |                |
                | x[x+, x]       | x > x+ | x-[x, x+, x] | x[x+,x++,x+]   |
                |                | x < x+ | reset        |                |
                |                |        |              |                |
                | x[x-, x--]     | x > x- | x+[x,x-,x--] | x[x-,x--,x---] |
                |                | x < x- | x-[x,x-,x--] | x[x+,x,x-]     |
                |                |        |              |                |
                | x[x+, x++, x+] | x > x+ | x-[x,x+,x++] | x[x+,x++,x+++] |
                |                | x < x+ | x+[x,x+,x++] | x[x-,x,x+]     |
                |                |        |              |                |
                | x[x+,x++,x+++] | x > x+ | x-[x,x+,x++] | x[x+,x++,x+++] |
                |                | x < x+ | x+[x,x+,x++] | x[x-,x,x+]     |
                |                |        |              |                |
                | x[x-,x--,x---] | x > x- | x+[x,x-,x--] | x[x-,x--,x---] |
                |                | x < x- | x-[x,x-,x--] | x[x+,x,x-]     |
                |                |        |              |                |
                | x[x-,x,x+]     | x > x- | stable       |                |
                |                | x < x- | reset        |                |
                |                |        |              |                |
                | x[x+,x,x-]     | x > x+ | stable       |                |
                |                | x < x+ | reset        |                |
                */
                match &self.previous_measured_values.as_slice() {
                    [] => {
                        trace!("A");
                        self.concurrent_count += 1;
                    }
                    [prev] => {
                        if mean > prev.mean {
                            trace!("B");
                            self.concurrent_count += 1;
                        } else {
                            trace!("C");
                            self.concurrent_count = prev.concurrent_count;
                        }
                    }
                    [_pprev, prev] =>
                    {
                        #[allow(clippy::comparison_chain)]
                        if self.concurrent_count > prev.concurrent_count {
                            if mean > prev.mean {
                                trace!("E");
                                self.concurrent_count += 1;
                            } else {
                                trace!("F");
                                self.concurrent_count = prev.concurrent_count;
                            }
                        } else if self.concurrent_count < prev.concurrent_count {
                            if mean > prev.mean {
                                if self.concurrent_count == 1 {
                                    trace!("G");
                                    self.set_underpowered(cur_measurements);
                                } else {
                                    trace!("H");
                                    self.concurrent_count -= 1;
                                }
                            } else {
                                trace!("I");
                                trace!("Concurrency controller found contradiction; resetting");
                                self.state = ConcurrencyControllerState::Reset;
                            }
                        } else {
                            trace!("J");
                            error!("Unexpected state. This is a bug in Balter.");
                            self.state = ConcurrencyControllerState::Reset;
                        }
                    }
                    [.., ppprev, pprev, prev] => {
                        if self.concurrent_count == prev.concurrent_count {
                            trace!("K");
                            error!("Unexpected state. This is a bug in Balter.");
                            self.state = ConcurrencyControllerState::Reset;
                        } else {
                            // Normalize to center around 3, which lets us match nicely.
                            //  x--- = 0
                            //  x-- = 1
                            //  x- = 2
                            //  x = 3
                            //  x+ = 4
                            //  x++ = 5
                            //  x+++ = 6
                            let last_3 = [
                                (prev.concurrent_count + 3) - self.concurrent_count,
                                (pprev.concurrent_count + 3) - self.concurrent_count,
                                (ppprev.concurrent_count + 3) - self.concurrent_count,
                            ];
                            match last_3 {
                                [4, 5, 4] | [4, 5, 6] => {
                                    if mean > prev.mean {
                                        if self.concurrent_count == 1 {
                                            trace!("L");
                                            self.set_underpowered(cur_measurements);
                                        } else {
                                            trace!("M");
                                            self.concurrent_count -= 1;
                                        }
                                    } else {
                                        trace!("N");
                                        self.concurrent_count += 1;
                                    }
                                }
                                [2, 1, 0] => {
                                    if mean > prev.mean {
                                        trace!("O");
                                        self.concurrent_count += 1;
                                    } else {
                                        trace!("P");
                                        self.concurrent_count = prev.concurrent_count;
                                    }
                                }
                                [2, 3, 4] | [4, 3, 2] => {
                                    if mean > prev.mean {
                                        trace!("Q");
                                        self.set_underpowered(cur_measurements);
                                    } else {
                                        trace!("R");
                                        trace!(
                                            "Concurrency controller found contradiction; resetting"
                                        );
                                        self.state = ConcurrencyControllerState::Reset;
                                    }
                                }
                                _ => {
                                    trace!("S");
                                    error!("Bug in Balter concurrency controller.");
                                    self.state = ConcurrencyControllerState::Reset;
                                }
                            }
                        }
                    }
                }

                if self.state == ConcurrencyControllerState::Reset {
                    self.reset();
                }

                if self.concurrent_count != cur_measurements.concurrent_count {
                    trace!("Adjusting concurrency count to {}", self.concurrent_count);
                    self.samples.clear();
                    self.previous_measured_values.push(cur_measurements);
                }
            }
        } else if error.abs() > 0.25 {
            // TODO: We're triggering this too often.
            warn!("Way over TPS limits: {mean}, {error}");
        }
    }

    fn set_underpowered(&mut self, measurement: ConcurrencyMeasurements) {
        match &mut self.underpowered_counter {
            Some((underpowered_count, concurrent_count))
                if *concurrent_count == self.concurrent_count =>
            {
                if *underpowered_count > 5 {
                    let max_tps = (measurement.mean - measurement.std).floor();
                    info!(
                        "Server is underpowered. Capable of TPS mean={}, std={}. Reducing to {}.",
                        measurement.mean, measurement.std, max_tps
                    );
                    self.state = ConcurrencyControllerState::Underpowered(max_tps);
                } else {
                    *underpowered_count += 1;
                    self.reset();
                    info!(
                        "Server may be underpowered. Sampling more measurements. {:?}",
                        self.underpowered_counter
                    );
                }
            }
            _ => {
                self.underpowered_counter = Some((1, self.concurrent_count));
                self.reset();
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct ConcurrencyMeasurements {
    pub concurrent_count: u64,
    pub mean: f64,
    pub std: f64,
}

fn mean(samples: &VecDeque<f64>) -> f64 {
    let sum: f64 = samples.iter().sum();
    sum / samples.len() as f64
}

fn std(samples: &VecDeque<f64>) -> f64 {
    let mean = mean(samples);

    let n = samples.len() as f64;
    let v = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.);

    v.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ntest::timeout(100)]
    fn test_concurrency_controller_underpowered() {
        let mut c = ConcurrencyController::new(100.);

        loop {
            match c.concurrent_count {
                1 => c.push(1.),
                2 => c.push(2.),
                3 => c.push(3.),
                4 => c.push(4.),
                5 => c.push(3.),
                _ => panic!("Bug in concurrency controller."),
            }

            if c.is_underpowered().is_some() {
                assert_eq!(c.concurrent_count, 4);
                break;
            }
        }
    }

    #[test]
    #[ntest::timeout(100)]
    fn test_concurrency_controller() {
        let mut c = ConcurrencyController::new(100.);

        loop {
            c.push(2.);
            if c.concurrent_count == 2 {
                break;
            }
        }

        loop {
            c.push(5.);
            if c.concurrent_count == 3 {
                break;
            }
        }

        loop {
            c.push(100.);
            if c.is_stable() {
                break;
            }
        }
    }
}
