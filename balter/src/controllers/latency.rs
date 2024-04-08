use crate::controllers::Controller;
use balter_core::{SampleSet, BASELINE_TPS};
use std::num::NonZeroU32;
use std::time::Duration;

#[allow(unused)]
pub(crate) struct LatencyController {
    latency: Duration,
    quantile: f64,
    goal_tps: NonZeroU32,
}

impl LatencyController {
    pub fn new(latency: Duration, quantile: f64) -> Self {
        Self {
            latency,
            quantile,
            goal_tps: BASELINE_TPS,
        }
    }
}

impl Controller for LatencyController {
    fn initial_tps(&self) -> NonZeroU32 {
        BASELINE_TPS
    }

    fn limit(&mut self, _samples: &SampleSet) -> NonZeroU32 {
        todo!()
    }
}
