//! Scenario logic and constants
#[cfg(feature = "rt")]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
#[cfg(feature = "rt")]
use serde_with::{serde_as, DurationSeconds};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

mod concurrency_controller;
mod error_rate_controller;
mod goal_tps;
mod saturate;
mod tps_sampler;

/// The default error rate used for `.saturate()`
pub const DEFAULT_SATURATE_ERROR_RATE: f64 = 0.03;

/// The default error rate used for `.overload()`
pub const DEFAULT_OVERLOAD_ERROR_RATE: f64 = 0.80;

// TODO: We should _not_ need to use a Boxed future! Every single function call for any load
// testing is boxed which *sucks*. Unfortunately I haven't figured out how to appease the Type
// system.
pub(crate) type BoxedFut = Pin<Box<dyn Future<Output = ()> + Send>>;

// TODO: Have a separate builder
#[derive(Clone, Debug)]
#[cfg_attr(feature = "rt", cfg_eval::cfg_eval, serde_as)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub(crate) struct ScenarioConfig {
    pub name: String,
    #[cfg_attr(feature = "rt", serde_as(as = "DurationSeconds"))]
    pub duration: Duration,
    pub kind: ScenarioKind,
}

impl ScenarioConfig {
    fn new(name: &str) -> Self {
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
}

#[derive(Default, Clone, Copy, Debug)]
#[cfg_attr(feature = "rt", derive(Serialize, Deserialize))]
pub(crate) enum ScenarioKind {
    #[default]
    Once,
    Tps(u32),
    Saturate(f64),
}

/// Load test scenario structure
///
/// Handler for running scenarios. Not intended for manual creation, use the [`#[scenario]`](balter_macros::scenario) macro which will add these methods to functions.
#[pin_project::pin_project]
pub struct Scenario {
    fut: fn() -> BoxedFut,
    runner_fut: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
    config: ScenarioConfig,
}

impl Scenario {
    #[doc(hidden)]
    pub fn new(name: &str, fut: fn() -> BoxedFut) -> Self {
        Self {
            fut,
            runner_fut: None,
            config: ScenarioConfig::new(name),
        }
    }

    /// Run the scenario increasing TPS until an error rate of 3% is reached.
    ///
    /// NOTE: Must supply a `.duration()` as well
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         .saturate()
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    pub fn saturate(mut self) -> Self {
        self.config.kind = ScenarioKind::Saturate(DEFAULT_SATURATE_ERROR_RATE);
        self
    }

    /// Run the scenario increasing TPS until an error rate of 80% is reached.
    ///
    /// NOTE: Must supply a `.duration()` as well
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         .overload()
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    pub fn overload(mut self) -> Self {
        self.config.kind = ScenarioKind::Saturate(DEFAULT_OVERLOAD_ERROR_RATE);
        self
    }

    /// Run the scenario increasing TPS until a custom error rate is reached.
    ///
    /// NOTE: Must supply a `.duration()` as well
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         .error_rate(0.25) // 25% error rate
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    pub fn error_rate(mut self, error_rate: f64) -> Self {
        self.config.kind = ScenarioKind::Saturate(error_rate);
        self
    }

    /// Run the scenario at the specified TPS.
    ///
    /// NOTE: Must supply a `.duration()` as well
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         .tps(632)
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    pub fn tps(mut self, tps: u32) -> Self {
        self.config.kind = ScenarioKind::Tps(tps);
        self
    }

    /// Run the scenario for the given duration.
    ///
    /// NOTE: Must include one of `.tps()`/`.saturate()`/`.overload()`/`.error_rate()`
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         .tps(10)
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    pub fn duration(mut self, duration: Duration) -> Self {
        self.config.duration = duration;
        self
    }

    #[cfg(feature = "rt")]
    pub(crate) fn set_config(mut self, config: ScenarioConfig) -> Self {
        self.config = config;
        self
    }
}

impl Future for Scenario {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // TODO: Surely there is a cleaner way to do this...
        if self.runner_fut.is_none() {
            let fut = self.fut;
            let config = self.config.clone();
            // TODO: There must be a way to run this future without boxing it. I feel like I'm
            // missing something really simple here.
            self.runner_fut = Some(Box::pin(async move { run_scenario(fut, config).await }));
        }

        if let Some(runner) = &mut self.runner_fut {
            runner.as_mut().poll(cx)
        } else {
            unreachable!()
        }
    }
}

async fn run_scenario(scenario: fn() -> BoxedFut, config: ScenarioConfig) {
    match config.kind {
        ScenarioKind::Once => scenario().await,
        ScenarioKind::Tps(_) => goal_tps::run_tps(scenario, config).await,
        ScenarioKind::Saturate(_) => saturate::run_saturate(scenario, config).await,
    }
}
