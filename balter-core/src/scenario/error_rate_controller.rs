use super::{concurrency_controller::ConcurrencyController, tps_sampler::TpsData};
use std::collections::VecDeque;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

const STARTING_TPS: f64 = 256.;

// TODO: Calculate experimentally
const MAX_SAMPLE_COUNT: usize = 10;
const MIN_SAMPLE_COUNT: usize = 5;

pub(crate) struct ErrorRateController {
    // NOTE: Samples are most recent first
    samples: VecDeque<TpsData>,
    goal_tps: f64,
    state: ErrorRateState,
    error_rate: f64,
    cc: ConcurrencyController,
}

impl ErrorRateController {
    pub fn new(error_rate: f64) -> Self {
        // TODO: Make an error
        assert!(error_rate > 0.);
        let cc = ConcurrencyController::new(STARTING_TPS);

        Self {
            samples: VecDeque::new(),
            goal_tps: STARTING_TPS,
            state: ErrorRateState::BigStep,
            error_rate,
            cc,
        }
    }

    pub fn goal_tps(&self) -> f64 {
        self.goal_tps
    }

    pub fn concurrency_count(&self) -> usize {
        self.cc.concurrent_count()
    }

    pub fn is_underpowered(&self) -> bool {
        self.state == ErrorRateState::Underpowered
    }

    #[allow(unused)]
    pub fn is_stable(&self) -> bool {
        self.state == ErrorRateState::Stable
    }

    pub fn push(&mut self, sample: TpsData) {
        self.cc.push(sample.tps());
        if self.cc.is_stable() {
            self.samples.push_front(sample);
            if self.samples.len() > MAX_SAMPLE_COUNT {
                let _ = self.samples.pop_back();
            }

            if self.samples.len() > MIN_SAMPLE_COUNT {
                #[allow(clippy::collapsible_if)]
                if self.analyze() {
                    self.cc.set_goal_tps(self.goal_tps);
                    self.clear();
                }
            }
        } else if let Some(max_tps) = self.cc.is_underpowered() {
            // TODO: We are not handling the case where we can't achieve the goal TPS set here, but
            // the goal TPS being set is too high for the error rate. This _will_ self-heal, but it
            // will put out an unecessary ping to the network if run distributed.
            self.state = ErrorRateState::Underpowered;
            self.goal_tps = max_tps;
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    fn analyze(&mut self) -> bool {
        let mean_error_rate: f64 = self.samples.iter().map(|x| x.error_rate()).sum();
        let mean_error_rate = mean_error_rate / self.samples.len() as f64;

        let diff: f64 = self.error_rate - mean_error_rate;

        if mean_error_rate == 0.0 {
            debug!(
                "Error rate of 0% with goal {}%; increasing TPS.",
                self.error_rate * 100.
            );
            // NOTE: Need to special-case 0.0 since it is the inflection point.
            self.increase_tps();
            true
        } else if diff.abs() < 0.05 {
            self.stabalize();
            false
        } else if diff.is_sign_positive() {
            debug!(
                "Error rate of {:.2}% with goal {}%; increasing TPS.",
                mean_error_rate * 100.,
                self.error_rate * 100.
            );
            self.increase_tps();
            true
        } else {
            debug!(
                "Error rate of {:.2}% with goal {}%; decreasing TPS.",
                mean_error_rate * 100.,
                self.error_rate * 100.
            );
            self.decrease_tps();
            true
        }
    }

    pub fn stabalize(&mut self) {
        if self.state != ErrorRateState::Stable {
            self.state = ErrorRateState::Stable;
            debug!(
                "Error rate controller is stable. Goal: {:.2}%, acheiving: {:.2}% at {:.2} TPS.",
                self.error_rate * 100.,
                self.samples[0].error_rate() * 100.,
                self.goal_tps,
            );
        }
    }

    pub fn increase_tps(&mut self) {
        match self.state {
            ErrorRateState::Underpowered => {}
            ErrorRateState::BigStep => {
                self.goal_tps *= 2.;
            }
            ErrorRateState::SmallStep => {
                self.goal_tps += (self.goal_tps * 0.1).floor();
            }
            ErrorRateState::Stable => {
                self.state = ErrorRateState::SmallStep;
                self.increase_tps();
            }
        }
    }

    pub fn decrease_tps(&mut self) {
        match self.state {
            ErrorRateState::Underpowered => {
                self.state = ErrorRateState::SmallStep;
                self.decrease_tps();
            }
            ErrorRateState::BigStep => {
                self.goal_tps /= 2.;
                self.state = ErrorRateState::SmallStep;
            }
            ErrorRateState::SmallStep => {
                self.goal_tps -= (self.goal_tps * 0.1).floor();
            }
            ErrorRateState::Stable => {
                self.state = ErrorRateState::SmallStep;
                self.decrease_tps();
            }
        }
    }
}

#[derive(PartialEq, Debug)]
enum ErrorRateState {
    BigStep,
    SmallStep,
    Stable,
    Underpowered,
}
