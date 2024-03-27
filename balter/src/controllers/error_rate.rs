use crate::controllers::Controller;
use balter_core::{SampleSet, TpsData, BASELINE_TPS};
use std::num::NonZeroU32;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

const ERROR_RATE_TOLERANCE: f64 = 0.03;
const DEFAULT_SMALL_STEP_SIZE: f64 = 0.1;

pub(crate) struct ErrorRateController {
    goal_tps: NonZeroU32,
    error_rate: f64,
    state: State,
}

impl ErrorRateController {
    pub fn new(error_rate: f64) -> Self {
        Self {
            goal_tps: BASELINE_TPS,
            error_rate,
            state: State::BigStep,
        }
    }

    fn check_in_bounds(&self, sample_error_rate: f64) -> Bounds {
        let bounds = (
            self.error_rate - ERROR_RATE_TOLERANCE,
            self.error_rate + ERROR_RATE_TOLERANCE,
        );
        let bounds = (bounds.0.max(0.), bounds.1.min(0.99));

        match sample_error_rate {
            // NOTE: Special case for 0. error rate since that is the inflection point
            x if x == 0. => Bounds::Under,
            x if x >= bounds.0 && x <= bounds.1 => Bounds::At,
            x if x > bounds.1 => Bounds::Over,
            _ => Bounds::Under,
        }
    }
}

impl Controller for ErrorRateController {
    fn initial_tps(&self) -> NonZeroU32 {
        BASELINE_TPS
    }

    fn limit(&mut self, samples: &SampleSet<TpsData>) -> NonZeroU32 {
        // TODO: Remove panic; this can be a type-safe check
        let sample_error_rate = samples.mean_err().expect("Invalid number of samples");

        match self.check_in_bounds(sample_error_rate) {
            Bounds::Under => match self.state {
                State::BigStep => {
                    self.goal_tps = NonZeroU32::new(self.goal_tps.get() * 2).unwrap();
                    self.goal_tps
                }
                State::SmallStep(_step_size) => {
                    todo!()
                }
                State::Stable => {
                    todo!()
                }
            },
            Bounds::At => {
                match self.state {
                    State::BigStep | State::SmallStep(_) => {
                        self.state = State::Stable;
                        // TODO: Remove unwraps
                        let samples_tps = samples.mean_tps().unwrap();
                        self.goal_tps = convert_to_nonzerou32(samples_tps).unwrap();
                        self.goal_tps
                    }
                    State::Stable => self.goal_tps,
                }
            }
            Bounds::Over => {
                match self.state {
                    State::BigStep => {
                        self.state = State::SmallStep(DEFAULT_SMALL_STEP_SIZE);
                        // TODO: Remove unwrap
                        self.goal_tps = NonZeroU32::new(self.goal_tps.get() / 2).unwrap();
                        self.goal_tps
                    }
                    State::SmallStep(step_size) => {
                        self.state = State::SmallStep(step_size / 2.);

                        todo!();

                        self.goal_tps
                    }
                    State::Stable => {
                        self.state = State::SmallStep(DEFAULT_SMALL_STEP_SIZE);
                        self.goal_tps
                    }
                }
            }
        }
    }
}

enum State {
    BigStep,
    SmallStep(f64),
    Stable,
}

enum Bounds {
    Under,
    At,
    Over,
}

fn convert_to_nonzerou32(val: f64) -> Option<NonZeroU32> {
    let val = val as u32;
    NonZeroU32::new(val)
}
