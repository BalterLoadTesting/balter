use super::ScenarioConfig;
use crate::controllers::concurrency::{ConcurrencyController, Message};
use crate::tps_sampler::TpsSampler;
use balter_core::stats::RunStatistics;
#[cfg(feature = "rt")]
use balter_runtime::runtime::BALTER_OUT;
use std::future::Future;
use std::num::NonZeroU32;
#[allow(unused_imports)]
use std::time::{Duration, Instant};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_tps<T, F>(scenario: T, config: ScenarioConfig) -> RunStatistics
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let goal_tps = config.goal_tps().unwrap();
    let mut controller = ConcurrencyController::new(NonZeroU32::new(goal_tps).unwrap());
    let mut sampler = TpsSampler::new(scenario, NonZeroU32::new(goal_tps).unwrap());
    sampler.set_concurrent_count(controller.concurrency());

    // NOTE: This loop is time-sensitive. Any long awaits or blocking will throw off measurements
    loop {
        let sample = sampler.sample_tps().await;
        if start.elapsed() > config.duration {
            break;
        }

        match controller.analyze(sample.tps()) {
            Message::None | Message::Stable => {}
            Message::AlterConcurrency(val) => {
                sampler.set_concurrent_count(val);
            }
            Message::TpsLimited(max_tps) => {
                sampler.set_tps_limit(max_tps);

                #[cfg(feature = "rt")]
                distribute_work(&config, start.elapsed(), u32::from(max_tps) as f64).await;
            }
        }
    }
    sampler.wait_for_shutdown().await;

    info!("Scenario complete");

    RunStatistics {
        concurrency: controller.concurrency(),
        goal_tps: controller.goal_tps(),
        stable: controller.is_stable(),
    }
}

#[cfg(feature = "rt")]
async fn distribute_work(config: &ScenarioConfig, elapsed: Duration, self_tps: f64) {
    let mut new_config = config.clone();
    // TODO: This does not take into account transmission time. Logic will have
    // to be far fancier to properly time-sync various peers on a single
    // scenario.
    new_config.duration = config.duration - elapsed;

    let new_tps = new_config.goal_tps().unwrap() - self_tps as u32;
    new_config.set_goal_tps(new_tps);

    tokio::spawn(async move {
        let (ref tx, _) = *BALTER_OUT;
        // TODO: Handle the error case.
        let _ = tx.send(new_config).await;
    });
}
