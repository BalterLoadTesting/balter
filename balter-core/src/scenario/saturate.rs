use super::{error_rate_controller::ErrorRateController, tps_sampler::TpsSampler};
use super::{BoxedFut, ScenarioConfig};
#[cfg(feature = "rt")]
use crate::runtime::BALTER_OUT;
#[allow(unused_imports)]
use std::time::{Duration, Instant};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_saturate(scenario: fn() -> BoxedFut, config: ScenarioConfig) {
    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let error_rate = config.error_rate().unwrap();
    let mut controller = ErrorRateController::new(error_rate);
    let mut sampler = TpsSampler::new(scenario, controller.goal_tps());

    let mut underpowered = false;

    // NOTE: This loop is time-sensitive. Any long awaits or blocking will throw off measurements
    while let Some(sample) = sampler.sample_tps().await {
        if start.elapsed() > config.duration {
            break;
        }

        controller.push(sample);
        sampler.set_tps_limit(controller.goal_tps());
        sampler.set_concurrent_count(controller.concurrency_count());

        if !underpowered && controller.is_underpowered() {
            underpowered = true;

            #[cfg(not(feature = "rt"))]
            error!("Current server is not powerful enough to reach TPS required to achieve error rate.");

            #[cfg(feature = "rt")]
            distribute_work(&config, start.elapsed()).await;
        }
    }

    info!("Scenario complete");
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
