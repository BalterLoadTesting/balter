use crate::controllers::Controller;
use balter_core::SampleSet;
use std::num::NonZeroU32;

pub(crate) struct ConstantController {
    goal_tps: NonZeroU32,
}

impl ConstantController {
    pub fn new(goal_tps: NonZeroU32) -> Self {
        Self { goal_tps }
    }
}

impl Controller for ConstantController {
    fn initial_tps(&self) -> NonZeroU32 {
        self.goal_tps
    }

    fn limit(&mut self, _samples: &SampleSet) -> NonZeroU32 {
        self.goal_tps
    }
}
