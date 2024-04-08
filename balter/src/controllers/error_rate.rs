use crate::controllers::Controller;
use balter_core::{SampleSet, BASELINE_TPS};
use std::num::NonZeroU32;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

const ERROR_RATE_TOLERANCE: f64 = 0.03;
const DEFAULT_SMALL_STEP_SIZE: f64 = 0.5;

pub(crate) struct ErrorRateController {
    base_label: String,
    goal_tps: NonZeroU32,
    error_rate: f64,
    state: State,
}

impl ErrorRateController {
    pub fn new(name: &str, error_rate: f64) -> Self {
        Self {
            base_label: format!("balter_{name}"),
            goal_tps: BASELINE_TPS,
            error_rate,
            state: State::BigStep,
        }
    }

    fn check_bounds(&self, sample_error_rate: f64) -> Bounds {
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

    fn limit(&mut self, samples: &SampleSet, stable: bool) -> NonZeroU32 {
        // TODO: Remove panic; this can be a type-safe check
        let sample_error_rate = samples.mean_err().expect("Invalid number of samples");

        let (new_goal_tps, new_state) = match self.check_bounds(sample_error_rate) {
            Bounds::Under => match self.state {
                s @ State::BigStep => {
                    trace!("Under bounds w/ BigStep");
                    (NonZeroU32::new(self.goal_tps.get() * 2).unwrap(), s)
                }
                s @ State::SmallStep(step_ratio) => {
                    trace!("Under bounds w/ SmallStep.");
                    // TODO: Better handling of conversions
                    let step = (self.goal_tps.get() as f64 * step_ratio).max(1.);
                    (
                        NonZeroU32::new(self.goal_tps.get() + step as u32).unwrap(),
                        s,
                    )
                }
                State::Stable => {
                    trace!("Under bounds w/ Stable.");
                    (self.goal_tps, State::SmallStep(DEFAULT_SMALL_STEP_SIZE))
                }
            },
            Bounds::At => {
                match self.state {
                    State::BigStep | State::SmallStep(_) => {
                        trace!("At bounds w/ BigStep|SmallStep.");
                        // TODO: Remove unwraps
                        let samples_tps = samples.mean_tps().unwrap();
                        (convert_to_nonzerou32(samples_tps).unwrap(), State::Stable)
                    }
                    s @ State::Stable => {
                        trace!("At bounds w/ Stable.");
                        (self.goal_tps, s)
                    }
                }
            }
            Bounds::Over => {
                match self.state {
                    State::BigStep => {
                        trace!("Over bounds w/ BigStep.");
                        // TODO: Remove unwrap
                        (
                            NonZeroU32::new(self.goal_tps.get() / 2).unwrap(),
                            State::SmallStep(DEFAULT_SMALL_STEP_SIZE),
                        )
                    }
                    State::SmallStep(step_ratio) => {
                        trace!("Over bounds w/ SmallStep.");

                        let step = (self.goal_tps.get() as f64 * step_ratio).max(1.);
                        (
                            NonZeroU32::new(self.goal_tps.get() - step as u32).unwrap(),
                            State::SmallStep(step_ratio / 2.),
                        )
                    }
                    State::Stable => {
                        trace!("Over bounds w/ Stable.");
                        (self.goal_tps, State::SmallStep(DEFAULT_SMALL_STEP_SIZE))
                    }
                }
            }
        };

        if new_goal_tps < self.goal_tps || stable {
            self.goal_tps = new_goal_tps;
            self.state = new_state;
        } else {
            debug!("TPS not stabalized; holding off on increasing Goal TPS");
        }

        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_erc_goal_tps", &self.base_label)).set(self.goal_tps.get());
            metrics::gauge!(format!("{}_erc_state", &self.base_label)).set(match self.state {
                State::BigStep => 2,
                State::SmallStep(_) => 1,
                State::Stable => 0,
            });
        }

        self.goal_tps
    }
}

#[derive(Debug, Clone, Copy)]
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
