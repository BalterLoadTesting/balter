//! Scenario logic and constants
use crate::controllers::{CompositeController, Controller};
use balter_core::{RunStatistics, ScenarioConfig};
#[cfg(feature = "rt")]
use balter_runtime::runtime::{RuntimeMessage, BALTER_OUT};
use std::{
    future::Future,
    num::NonZeroU32,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

mod sampler;

use sampler::ConcurrentSampler;

/// Load test scenario structure
///
/// Handler for running scenarios. Not intended for manual creation, use the [`#[scenario]`](balter_macros::scenario) macro which will add these methods to functions.
#[pin_project::pin_project]
pub struct Scenario<T> {
    func: T,
    runner_fut: Option<Pin<Box<dyn Future<Output = RunStatistics> + Send>>>,
    config: ScenarioConfig,
}

impl<T> Scenario<T> {
    #[doc(hidden)]
    pub fn new(name: &str, func: T) -> Self {
        Self {
            func,
            runner_fut: None,
            config: ScenarioConfig::new(name),
        }
    }
}

impl<T, F> Future for Scenario<T>
where
    T: Fn() -> F + Send + 'static + Clone + Sync,
    F: Future<Output = ()> + Send,
{
    type Output = RunStatistics;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.runner_fut.is_none() {
            let func = self.func.clone();
            let config = self.config.clone();
            self.runner_fut = Some(Box::pin(async move { run_scenario(func, config).await }));
        }

        if let Some(runner) = &mut self.runner_fut {
            runner.as_mut().poll(cx)
        } else {
            unreachable!()
        }
    }
}

pub trait ConfigurableScenario<T: Send>: Future<Output = T> + Sized + Send {
    fn error_rate(self, error_rate: f64) -> Self;
    fn tps(self, tps: u32) -> Self;
    fn latency(self, latency: Duration, quantile: f64) -> Self;
    fn duration(self, duration: Duration) -> Self;
}

impl<T, F> ConfigurableScenario<RunStatistics> for Scenario<T>
where
    T: Fn() -> F + Send + 'static + Clone + Sync,
    F: Future<Output = ()> + Send,
{
    /// Run the scenario at the specified TPS.
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         // Scale scenario until 5K TPS
    ///         .tps(5_000)
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the provided TPS is zero
    fn tps(mut self, tps: u32) -> Self {
        self.config.max_tps =
            Some(NonZeroU32::new(tps).expect("TPS provided must be non-zero. Given: {tps}"));
        self
    }

    /// Run the scenario increasing TPS until a custom error rate is reached.
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         // Scale scenario until 25% error rate
    ///         .error_rate(0.25)
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the error_rate is not between 0 and 1.
    fn error_rate(mut self, error_rate: f64) -> Self {
        if !(0. ..=1.).contains(&error_rate) {
            panic!(
                "Specified error rate must be between 0 and 1. Value provided was {error_rate}."
            );
        }
        self.config.error_rate = Some(error_rate);
        self
    }

    /// Run the scenario up to the specified latency, given a quantile.
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    /// use std::num::NonZeroU32;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         // Scale scenario until p95 latency is 200ms
    ///         .latency(Duration::from_millis(200), 0.95)
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if the quantile is not between 0 and 1.
    fn latency(mut self, latency: Duration, quantile: f64) -> Self {
        if !(0. ..=1.).contains(&quantile) {
            panic!("Specified quantile must be between 0 and 1. Value provided was {quantile}.");
        }

        self.config.latency = Some((latency, quantile));
        self
    }

    /// Run the scenario for the given duration.
    ///
    /// NOTE: This method doesn't make much sense without one of the other
    /// load-testing methods (`tps()`/`error_rate()`/`latency()`)
    ///
    /// # Example
    /// ```no_run
    /// use balter::prelude::*;
    /// use std::time::Duration;
    /// use std::num::NonZeroU32;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     my_scenario()
    ///         .tps(10_000)
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    fn duration(mut self, duration: Duration) -> Self {
        self.config.duration = Some(duration);
        self
    }
}

#[cfg(feature = "rt")]
mod runtime {
    use super::*;
    use balter_runtime::DistributedScenario;

    impl<T, F> DistributedScenario for Scenario<T>
    where
        T: Fn() -> F + Send + 'static + Clone + Sync,
        F: Future<Output = ()> + Send,
    {
        #[allow(unused)]
        fn set_config(
            &self,
            config: ScenarioConfig,
        ) -> Pin<Box<dyn DistributedScenario<Output = Self::Output>>> {
            Box::pin(Scenario {
                func: self.func.clone(),
                runner_fut: None,
                config,
            })
        }
    }
}

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_scenario<T, F>(scenario: T, config: ScenarioConfig) -> RunStatistics
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    if config.is_unconfigured() {
        debug!(
            "Not load testing {} with config {:?}, because it has no work to do.",
            config.name, &config
        );
        return RunStatistics::default();
    }

    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let mut controllers = CompositeController::new(&config);
    let mut sampler = ConcurrentSampler::new(&config.name, scenario, controllers.initial_tps());

    // NOTE: This loop is time-sensitive. Any long awaits or blocking will throw off measurements
    loop {
        if let (stable, Some(samples)) = sampler.get_samples().await {
            // NOTE: We have our break-out inside this branch so that our final sampler_stats are
            // accurate.
            if let Some(duration) = config.duration {
                if start.elapsed() > duration {
                    break;
                }
            }

            let new_goal_tps = controllers.limit(samples, stable);

            if new_goal_tps < sampler.goal_tps() || stable {
                sampler.set_goal_tps(new_goal_tps);
            }
        }
    }
    let sampler_stats = sampler.wait_for_shutdown().await;

    #[cfg(feature = "rt")]
    signal_completion().await;

    info!("Scenario complete");

    RunStatistics {
        concurrency: sampler_stats.concurrency,
        goal_tps: sampler_stats.goal_tps.get(),
        actual_tps: sampler_stats.final_sample_set.mean_tps(),
        latency_p50: sampler_stats.final_sample_set.latency(0.5),
        latency_p90: sampler_stats.final_sample_set.latency(0.9),
        latency_p99: sampler_stats.final_sample_set.latency(0.99),
        error_rate: sampler_stats.final_sample_set.mean_err(),
        tps_limited: sampler_stats.tps_limited,
    }
}

#[allow(unused)]
#[cfg(feature = "rt")]
async fn distribute_work(_config: &ScenarioConfig, _elapsed: Duration, _self_tps: f64) {
    /*
    let mut new_config = config.clone();
    // TODO: This does not take into account transmission time. Logic will have
    // to be far fancier to properly time-sync various peers on a single
    // scenario.
    new_config.duration = config.duration - elapsed;

    let new_tps = new_config.goal_tps().unwrap() - self_tps as u32;
    new_config.set_goal_tps(new_tps);

    let (ref tx, _) = *BALTER_OUT;
    // TODO: Handle the error case.
    let _ = tx.send(RuntimeMessage::Help(new_config)).await;
    */
    todo!()
}

#[cfg(feature = "rt")]
async fn signal_completion() {
    // TODO: We should send which scenario was actually completed so that the runtime can be
    // intelligent about figuring out if load was alleviated or not.

    let (ref tx, _) = *BALTER_OUT;
    // TODO: Handle the error case.
    let _ = tx.send(RuntimeMessage::Finished).await;
}
