use super::ScenarioConfig;
use crate::controllers::error_rate::{ErrorRateController, Message};
use crate::tps_sampler::TpsSampler;
use balter_core::stats::RunStatistics;
#[cfg(feature = "rt")]
use balter_runtime::runtime::BALTER_OUT;
use std::future::Future;
#[allow(unused_imports)]
use std::time::{Duration, Instant};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_saturate<T, F>(scenario: T, config: ScenarioConfig) -> RunStatistics
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let error_rate = config.error_rate().unwrap();
    let mut controller = ErrorRateController::new(error_rate);
    let mut sampler = TpsSampler::new(scenario, controller.tps_limit());
    sampler.set_concurrent_count(controller.concurrency());

    loop {
        let sample = sampler.sample_tps().await;
        if start.elapsed() > config.duration {
            break;
        }

        match controller.analyze(sample) {
            Message::None | Message::Stable => {}
            Message::AlterConcurrency(val) => {
                sampler.set_concurrent_count(val);
            }
            Message::AlterTpsLimit(val) => {
                sampler.set_tps_limit(val);
            }
            Message::TpsLimited(max_tps) => {
                sampler.set_tps_limit(max_tps);

                #[cfg(feature = "rt")]
                distribute_work(&config, start.elapsed()).await;
            }
        }
    }
    sampler.wait_for_shutdown().await;

    info!("Scenario complete");

    RunStatistics {
        concurrency: controller.concurrency(),
        goal_tps: controller.tps_limit(),
        stable: controller.is_stable(),
    }
}

#[cfg(feature = "rt")]
async fn distribute_work(config: &ScenarioConfig, elapsed: Duration) {
    info!("Current server is not powerful enough; sending work to peers.");

    let mut new_config = config.clone();
    // TODO: This does not take into account transmission time. Logic will have
    // to be far fancier to properly time-sync various peers on a single
    // scenario.
    new_config.duration = config.duration - elapsed;

    tokio::spawn(async move {
        let (ref tx, _) = *BALTER_OUT;
        // TODO: Handle the error case.
        let _ = tx.send(new_config).await;
    });
}
