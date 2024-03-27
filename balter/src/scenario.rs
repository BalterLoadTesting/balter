//! Scenario logic and constants
use crate::controllers::{CompositeController, ConcurrencyController, Controller};
use crate::tps_sampler::TpsSampler;
use balter_core::{
    RunStatistics, SampleSet, ScenarioConfig, DEFAULT_OVERLOAD_ERROR_RATE,
    DEFAULT_SATURATE_ERROR_RATE,
};
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
    fn saturate(self) -> Self;
    fn overload(self) -> Self;
    fn error_rate(self, error_rate: f64) -> Self;
    fn tps(self, tps: NonZeroU32) -> Self;
    //fn direct(self, tps_limit: u32, concurrency: usize) -> Self;
    fn duration(self, duration: Duration) -> Self;
}

impl<T, F> ConfigurableScenario<RunStatistics> for Scenario<T>
where
    T: Fn() -> F + Send + 'static + Clone + Sync,
    F: Future<Output = ()> + Send,
{
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
    fn saturate(mut self) -> Self {
        self.config.error_rate = Some(DEFAULT_SATURATE_ERROR_RATE);
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
    fn overload(mut self) -> Self {
        self.config.error_rate = Some(DEFAULT_OVERLOAD_ERROR_RATE);
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
    fn error_rate(mut self, error_rate: f64) -> Self {
        self.config.error_rate = Some(error_rate);
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
    ///         .tps(NonZeroU32::new(632).unwrap())
    ///         .duration(Duration::from_secs(120))
    ///         .await;
    /// }
    ///
    /// #[scenario]
    /// async fn my_scenario() {
    /// }
    /// ```
    fn tps(mut self, tps: NonZeroU32) -> Self {
        self.config.max_tps = Some(tps);
        self
    }

    /*
    /// Run the scenario with direct control over TPS and concurrency.
    /// No automatic controls will limit or change any values. This is intended
    /// for development testing or advanced ussage.
    fn direct(mut self, tps_limit: u32, concurrency: usize) -> Self {
        self.config.kind = ScenarioKind::Direct(tps_limit, concurrency);
        self
    }
    */

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

const SAMPLE_WINDOW_SIZE: usize = 10;

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_scenario<T, F>(scenario: T, config: ScenarioConfig) -> RunStatistics
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let mut controllers = CompositeController::new(&config);
    // NOTE: Special case the concurrency controller because it is always active
    let mut cc = ConcurrencyController::new(controllers.initial_tps());
    let mut sampler = TpsSampler::new(scenario, controllers.initial_tps());
    sampler.set_concurrent_count(cc.concurrency());

    // NOTE: This loop is time-sensitive. Any long awaits or blocking will throw off measurements
    let mut samples = SampleSet::new(SAMPLE_WINDOW_SIZE);
    loop {
        let sample = sampler.sample_tps().await;
        samples.push(sample);

        if samples.full() {
            let _cc_res = cc.analyze(&samples);
            let goal_tps = controllers.limit(&samples);

            cc.set_goal_tps(goal_tps);
            sampler.set_tps_limit(goal_tps);
        }

        if let Some(duration) = config.duration {
            if start.elapsed() > duration {
                break;
            }
        }
    }
    sampler.wait_for_shutdown().await;

    info!("Scenario complete");

    #[cfg(feature = "rt")]
    signal_completion().await;

    // TODO: Fix
    RunStatistics {
        concurrency: cc.concurrency(),
        tps: NonZeroU32::new(1).unwrap(),
        stable: true,
    }
}

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
