use crate::BASELINE_TPS;
#[cfg(feature = "rt")]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
#[cfg(feature = "rt")]
use serde_with::{serde_as, DurationSeconds};
use std::num::NonZeroU32;
use std::time::Duration;

// TODO: Have a separate builder
#[doc(hidden)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "rt", cfg_eval::cfg_eval, serde_as)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub struct ScenarioConfig {
    pub name: String,
    #[cfg_attr(feature = "rt", serde_as(as = "Option<DurationSeconds>"))]
    pub duration: Option<Duration>,
    pub max_tps: Option<NonZeroU32>,
    pub error_rate: Option<f64>,
    pub latency: Option<Duration>,
}

impl ScenarioConfig {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            duration: None,
            max_tps: None,
            error_rate: None,
            latency: None,
        }
    }

    pub fn starting_tps(&self) -> Option<NonZeroU32> {
        match self {
            ScenarioConfig {
                error_rate: Some(_),
                ..
            }
            | ScenarioConfig {
                latency: Some(_), ..
            } => Some(BASELINE_TPS),

            ScenarioConfig {
                max_tps: Some(tps), ..
            } => Some(*tps),

            _ => None,
        }
    }

    #[allow(unused)]
    pub fn set_max_tps(&mut self, max_tps: NonZeroU32) {
        self.max_tps = Some(max_tps);
    }
}
