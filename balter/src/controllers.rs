mod constant;
mod error_rate;
mod latency;

pub(crate) use constant::ConstantController;
pub(crate) use error_rate::ErrorRateController;
pub(crate) use latency::LatencyController;

use crate::measurements::Measurements;
use balter_core::{LatencyConfig, ScenarioConfig};
use std::num::NonZeroU32;

pub(crate) trait Controller: Send {
    fn initial_tps(&self) -> NonZeroU32;
    fn limit(&mut self, sample: &Measurements, stable: bool) -> NonZeroU32;
}

pub(crate) struct CompositeController {
    controllers: Vec<Box<dyn Controller>>,
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
            )));
        }

        Self { controllers }
    }
}

impl Controller for CompositeController {
    fn initial_tps(&self) -> NonZeroU32 {
        self.controllers
            .iter()
            .map(|c| c.initial_tps())
            .min()
            .expect("No controllers present.")
    }

    fn limit(&mut self, sample: &Measurements, stable: bool) -> NonZeroU32 {
        self.controllers
            .iter_mut()
            .map(|c| c.limit(sample, stable))
            .min()
            .expect("No controllers present.")
    }
}
