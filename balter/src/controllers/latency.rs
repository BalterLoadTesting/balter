use crate::controllers::Controller;
use crate::measurement::Measurement;
use balter_core::BASE_TPS;
use std::num::NonZeroU32;
use std::time::Duration;
#[allow(unused)]
use tracing::{debug, error, trace};

const DEFAULT_KP: f64 = 0.9;

#[allow(unused)]
pub(crate) struct LatencyController {
    base_label: String,
    latency: Duration,
    quantile: f64,
    goal_tps: NonZeroU32,
    kp: f64,
}

impl LatencyController {
    pub fn new(name: &str, latency: Duration, quantile: f64, kp: Option<f64>) -> Self {
        let s = Self {
            base_label: format!("balter_{name}"),
            latency,
            quantile,
            goal_tps: BASE_TPS,
            kp: kp.unwrap_or(DEFAULT_KP),
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
        BASE_TPS
    }

    fn limit(&mut self, sample: &Measurement, stable: bool) -> NonZeroU32 {
        let measured_latency = sample.latency(self.quantile);

        trace!("LATENCY: Measured {measured_latency:?}");
        trace!("LATENCY: Expected {:?}", self.latency);

        let normalized_err = 1. - measured_latency.as_secs_f64() / self.latency.as_secs_f64();
        trace!("LATENCY: Error {normalized_err:?}");

        let new_goal = self.goal_tps.get() as f64 * (1. + self.kp * normalized_err);
        trace!("LATENCY: New Goal {new_goal:?}");

        if let Some(new_goal) = NonZeroU32::new(new_goal as u32) {
            if new_goal < self.goal_tps || stable {
                self.goal_tps = new_goal;
                self.goal_tps_metric();
            } else {
                debug!("TPS not stabalized; holding off on increasing TPS");
            }
        } else {
            error!("Error in the LatencyController. Calculated a goal_tps of {new_goal} which is invalid.");
        }

        self.goal_tps
    }
}
