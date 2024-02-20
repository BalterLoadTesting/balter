use crate::controllers::{
    concurrency::{ConcurrencyController, Message as CMessage},
    sample_set::SampleSet,
};
use crate::tps_sampler::TpsData;
use std::num::NonZeroU32;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

const STARTING_TPS: u32 = 256;

// TODO: Calculate experimentally
// NOTE: Must be _slightly_ less than ConcurrencyController to allow for adjusting for error rate
// before CC wipes the data
const WINDOW_SIZE: usize = 19;

pub(crate) struct ErrorRateController {
    samples: SampleSet<TpsData>,
    goal_tps: NonZeroU32,
    error_rate: f64,
    state: State,
    cc: ConcurrencyController,
}

impl ErrorRateController {
    pub fn new(error_rate: f64) -> Self {
        let goal_tps = NonZeroU32::new(STARTING_TPS).unwrap();
        Self {
            samples: SampleSet::new(WINDOW_SIZE),
            goal_tps,
            error_rate,
            state: State::BigStep,
            cc: ConcurrencyController::new(goal_tps),
        }
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.goal_tps
    }

    pub fn concurrency(&self) -> usize {
        self.cc.concurrency()
    }

    pub fn analyze(&mut self, sample: TpsData) -> Message {
        if sample.total() == 0 {
            error!("No transactions sampled");
            return Message::None;
        }

        self.samples.push(sample);

        match (self.cc.analyze(sample.tps()), self.analyze_inner()) {
            (_, None) => Message::None,
            (_, Some(AnalyzeResult::Over)) => {
                match self.state {
                    State::BigStep => {
                        self.goal_tps =
                            NonZeroU32::new(self.goal_tps.get() - (self.goal_tps.get() / 2))
                                .unwrap();
                    }
                    _ => {
                        // TODO: Clean up conversions
                        self.goal_tps = NonZeroU32::new(
                            (self.goal_tps.get() as f64 - self.goal_tps.get() as f64 * 0.05) as u32,
                        )
                        .unwrap();
                    }
                }
                self.state = State::SmallStep;
                self.cc.set_goal_tps(self.goal_tps);
                self.clear();
                debug!("Error rate is over the goal, adjusting TPS limit");
                Message::AlterTpsLimit(self.goal_tps)
            }
            (CMessage::Stable, Some(AnalyzeResult::At)) => {
                if !matches!(self.state, State::Stable(_)) {
                    debug!(
                        "ErrorRateController stabalized at {} TPS with {:.2}% error.",
                        self.goal_tps,
                        self.samples.mean_err().unwrap() * 100.
                    );
                }
                self.state = State::Stable(0);
                Message::Stable
            }
            (_, Some(AnalyzeResult::At)) => {
                self.state = State::SmallStep;
                self.goal_tps = NonZeroU32::new(self.samples.mean_tps().unwrap() as u32).unwrap();
                self.cc.set_goal_tps(self.goal_tps);
                self.clear();
                trace!("Potentially reached error rate at {} TPS.", self.goal_tps);
                Message::AlterTpsLimit(self.goal_tps)
            }
            (CMessage::None, Some(AnalyzeResult::Under)) => Message::None,
            (CMessage::Stable, Some(AnalyzeResult::Under)) => match self.state {
                State::BigStep => {
                    self.goal_tps = self.goal_tps.saturating_mul(NonZeroU32::new(2).unwrap());
                    self.cc.set_goal_tps(self.goal_tps);
                    self.clear();
                    trace!("Increasing TPS to {}", self.goal_tps);
                    Message::AlterTpsLimit(self.goal_tps)
                }
                State::SmallStep => {
                    self.goal_tps = self
                        .goal_tps
                        .saturating_add((self.goal_tps.get() as f64 * 0.05).ceil() as u32);
                    self.cc.set_goal_tps(self.goal_tps);
                    self.clear();
                    trace!("Increasing TPS to {}", self.goal_tps);
                    Message::AlterTpsLimit(self.goal_tps)
                }
                _ => {
                    self.state = State::SmallStep;
                    Message::None
                }
            },
            (CMessage::TpsLimited(tps_limit), Some(AnalyzeResult::Under)) => {
                self.state = State::Underpowered(tps_limit);
                self.goal_tps = tps_limit;
                self.clear();
                Message::TpsLimited(tps_limit)
            }
            (CMessage::AlterConcurrency(concurrency), Some(AnalyzeResult::Under)) => {
                self.clear();
                Message::AlterConcurrency(concurrency)
            }
        }
    }

    fn analyze_inner(&self) -> Option<AnalyzeResult> {
        let err = self.samples.mean_err()?;
        debug!("Error rate of {:.2}%", err * 100.);

        let bounds = (self.error_rate - 0.03, self.error_rate + 0.03);
        let bounds = (bounds.0.max(0.), bounds.1.min(0.99));

        debug!("Bounds used for comparison: ({}, {})", bounds.0, bounds.1);

        Some(match err {
            x if x == 0. => AnalyzeResult::Under,
            x if x >= bounds.0 && x <= bounds.1 => AnalyzeResult::At,
            x if x > bounds.1 => AnalyzeResult::Over,
            _ => {
                error!("Error in ErrorRateController, error rate of {err:.2?}");
                AnalyzeResult::Under
            }
        })
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum Message {
    None,
    Stable,
    AlterConcurrency(usize),
    AlterTpsLimit(NonZeroU32),
    TpsLimited(NonZeroU32),
}

enum AnalyzeResult {
    Under,
    Over,
    At,
}

enum State {
    BigStep,
    SmallStep,
    Stable(usize),
    Underpowered(NonZeroU32),
}
