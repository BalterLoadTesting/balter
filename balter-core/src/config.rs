#[cfg(feature = "rt")]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
#[cfg(feature = "rt")]
use serde_with::{serde_as, DurationSeconds};
use std::time::Duration;

// TODO: Have a separate builder
#[doc(hidden)]
#[derive(Clone, Debug)]
#[cfg_attr(feature = "rt", cfg_eval::cfg_eval, serde_as)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub struct ScenarioConfig {
    pub name: String,
    #[cfg_attr(feature = "rt", serde_as(as = "DurationSeconds"))]
    pub duration: Duration,
    pub kind: ScenarioKind,
}

impl ScenarioConfig {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            duration: Default::default(),
            kind: Default::default(),
        }
    }

    pub fn goal_tps(&self) -> Option<u32> {
        if let ScenarioKind::Tps(goal_tps) = self.kind {
            Some(goal_tps)
        } else {
            None
        }
    }

    #[allow(unused)]
    pub fn set_goal_tps(&mut self, new_goal_tps: u32) -> bool {
        if let ScenarioKind::Tps(goal_tps) = &mut self.kind {
            *goal_tps = new_goal_tps;
            true
        } else {
            false
        }
    }

    #[allow(unused)]
    pub fn error_rate(&self) -> Option<f64> {
        if let ScenarioKind::Saturate(error_rate) = self.kind {
            Some(error_rate)
        } else {
            None
        }
    }

    pub fn direct(&self) -> Option<(u32, usize)> {
        if let ScenarioKind::Direct(tps, concurrency) = self.kind {
            Some((tps, concurrency))
        } else {
            None
        }
    }
}

#[doc(hidden)]
#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub enum ScenarioKind {
    #[default]
    Once,
    Tps(u32),
    Saturate(f64),
    Direct(u32, usize),
}
