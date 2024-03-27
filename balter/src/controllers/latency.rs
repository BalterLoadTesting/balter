use crate::controllers::Controller;
use balter_core::SampleSet;
use std::num::NonZeroU32;
use std::time::Duration;

pub(crate) struct LatencyController {}

impl LatencyController {
    pub fn new(_latency: Duration) -> Self {
        Self {}
    }
}

impl Controller for LatencyController {
    fn initial_tps(&self) -> NonZeroU32 {
        todo!()
    }

    fn limit(&mut self, _samples: &SampleSet) -> NonZeroU32 {
        todo!()
    }
}
