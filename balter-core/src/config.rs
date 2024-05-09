use crate::BASE_TPS;
#[cfg(feature = "rt")]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
#[cfg(feature = "rt")]
use serde_with::{serde_as, DurationSecondsWithFrac};
use std::num::NonZeroU32;
use std::time::Duration;

// TODO: Have a separate builder
#[doc(hidden)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "rt", cfg_eval::cfg_eval, serde_as)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub struct ScenarioConfig {
    pub name: String,
    #[cfg_attr(feature = "rt", serde_as(as = "Option<DurationSecondsWithFrac>"))]
    pub duration: Option<Duration>,
    pub max_tps: Option<NonZeroU32>,
    pub error_rate: Option<f64>,
    pub latency: Option<LatencyConfig>,
    pub hints: HintConfig,
}

impl ScenarioConfig {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            duration: None,
            max_tps: None,
            error_rate: None,
            latency: None,
            hints: HintConfig::default(),
        }
    }

    pub fn is_unconfigured(&self) -> bool {
        // NOTE: Technically just setting `duration` should do _something_,
        // but its realistically an edge-case.
        #[allow(clippy::match_like_matches_macro)]
        match (self.max_tps, self.error_rate, self.latency) {
            (None, None, None) => true,
            _ => false,
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
            } => Some(BASE_TPS),

            ScenarioConfig {
                max_tps: Some(tps), ..
            } => Some(*tps),

            _ => None,
        }
    }

    pub fn concurrency(&self) -> usize {
        self.hints.concurrency
    }

    #[allow(unused)]
    pub fn set_max_tps(&mut self, max_tps: NonZeroU32) {
        self.max_tps = Some(max_tps);
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Copy)]
#[cfg_attr(feature = "rt", cfg_eval::cfg_eval, serde_as)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub struct LatencyConfig {
    #[cfg_attr(feature = "rt", serde_as(as = "DurationSecondsWithFrac"))]
    pub latency: Duration,
    pub quantile: f64,
}

impl LatencyConfig {
    pub fn new(latency: Duration, quantile: f64) -> Self {
        Self { latency, quantile }
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Copy)]
#[cfg_attr(feature = "rt", cfg_eval::cfg_eval, serde_as)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub struct HintConfig {
    pub concurrency: usize,
}

impl Default for HintConfig {
    fn default() -> Self {
        Self {
            concurrency: crate::BASE_CONCURRENCY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_config_serialization() {
        insta::assert_json_snapshot!(ScenarioConfig {
            name: "test_scenario".to_string(),
            duration: Some(Duration::from_secs(300)),
            max_tps: Some(NonZeroU32::new(2_000).unwrap()),
            error_rate: Some(0.03),
            latency: Some(LatencyConfig::new(Duration::from_millis(20), 0.99)),
            hints: HintConfig::default(),
        });
    }
}
