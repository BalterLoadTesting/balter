mod constant;
mod error_rate;
mod latency;

pub(crate) use constant::ConstantController;
pub(crate) use error_rate::ErrorRateController;
pub(crate) use latency::LatencyController;

use crate::measurement::Measurement;
use balter_core::{LatencyConfig, ScenarioConfig};
use std::num::NonZeroU32;

pub(crate) trait Controller: Send {
    fn initial_tps(&self) -> NonZeroU32;
    fn limit(&mut self, sample: &Measurement, stable: bool) -> NonZeroU32;
}

pub(crate) struct CompositeController {
    controllers: Vec<Box<dyn Controller>>,
    starting_tps: Option<NonZeroU32>,
}

impl CompositeController {
    pub fn new(config: &ScenarioConfig) -> Self {
        let mut controllers = vec![];

        if let Some(tps) = config.max_tps {
            controllers.push(Box::new(ConstantController::new(tps)) as Box<dyn Controller>);
        }

        if let Some(error_rate) = config.error_rate {
            controllers.push(Box::new(ErrorRateController::new(&config.name, error_rate)));
        }

        if let Some(LatencyConfig { latency, quantile }) = config.latency {
            controllers.push(Box::new(LatencyController::new(
                &config.name,
                latency,
                quantile,
                config.hints.latency_controller,
            )));
        }

        let starting_tps = config.hints.starting_tps;

        Self {
            controllers,
            starting_tps,
        }
    }
}

impl Controller for CompositeController {
    fn initial_tps(&self) -> NonZeroU32 {
        if let Some(tps) = self.starting_tps {
            tps
        } else {
            self.controllers
                .iter()
                .map(|c| c.initial_tps())
                .min()
                .expect("No controllers present.")
        }
    }

    fn limit(&mut self, sample: &Measurement, stable: bool) -> NonZeroU32 {
        self.controllers
            .iter_mut()
            .map(|c| c.limit(sample, stable))
            .min()
            .expect("No controllers present.")
    }
}
