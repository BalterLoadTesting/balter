use crate::controllers::Controller;
use balter_core::{SampleSet, BASELINE_TPS};
use std::num::NonZeroU32;
use std::time::Duration;
use tracing::error;

const KP: f64 = 1.2;

#[allow(unused)]
pub(crate) struct LatencyController {
    base_label: String,
    latency: Duration,
    quantile: f64,
    goal_tps: NonZeroU32,
}

impl LatencyController {
    pub fn new(name: &str, latency: Duration, quantile: f64) -> Self {
        let s = Self {
            base_label: format!("balter_{name}"),
            latency,
            quantile,
            goal_tps: BASELINE_TPS,
        };
        s.goal_tps_metric();
        s
    }

    fn goal_tps_metric(&self) {
        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_lc_goal_tps", &self.base_label)).set(self.goal_tps.get());
        }
    }
}

impl Controller for LatencyController {
    fn initial_tps(&self) -> NonZeroU32 {
        BASELINE_TPS
    }

    fn limit(&mut self, samples: &SampleSet) -> NonZeroU32 {
        let measured_latency = samples.latency(self.quantile);

        let normalized_err = 1. - measured_latency.as_secs_f64() / self.latency.as_secs_f64();

        let new_goal = self.goal_tps.get() as f64 * (1. + KP * normalized_err);

        if let Some(new_goal) = NonZeroU32::new(new_goal as u32) {
            self.goal_tps = new_goal;
            self.goal_tps_metric();
        } else {
            error!("Error in the LatencyController. Calculated a goal_tps of {new_goal} which is invalid.");
        }

        self.goal_tps
    }
}
