use super::{concurrency_controller::ConcurrencyController, tps_sampler::TpsSampler};
use super::{BoxedFut, ScenarioConfig};
#[cfg(feature = "rt")]
use crate::runtime::BALTER_OUT;
#[allow(unused_imports)]
use std::time::{Duration, Instant};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_tps(scenario: fn() -> BoxedFut, config: ScenarioConfig) {
    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let goal_tps = config.goal_tps().unwrap();
    let mut controller = ConcurrencyController::new(goal_tps as f64);
    let mut sampler = TpsSampler::new(scenario, goal_tps as f64).await;

    // NOTE: This loop is time-sensitive. Any long awaits or blocking will throw off measurements
    while let Some(sample) = sampler.sample_tps().await {
        if start.elapsed() > config.duration {
            break;
        }

        controller.push(sample.tps());
        sampler.set_concurrent_count(controller.concurrent_count());

        if let Some(max_tps) = controller.is_underpowered() {
            controller.set_goal_tps(max_tps);
            sampler.set_tps_limit(max_tps);

            #[cfg(feature = "rt")]
            distribute_work(&config, start.elapsed(), max_tps).await;
        }
    }

    info!("Scenario complete");
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
