use super::{BoxedFut, ScenarioConfig};
use crate::sampling::tps_sampler::TpsSampler;
use humantime::format_duration;
use std::num::NonZeroU32;
#[allow(unused_imports)]
use std::time::{Duration, Instant};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn, Instrument};

#[instrument(name="scenario", skip_all, fields(name=config.name))]
pub(crate) async fn run_direct(scenario: fn() -> BoxedFut, config: ScenarioConfig) {
    info!("Running {} with config {:?}", config.name, &config);

    let start = Instant::now();

    let (goal_tps, concurrency) = config.direct().unwrap();
    let mut sampler = TpsSampler::new(scenario, NonZeroU32::new(goal_tps).unwrap());
    sampler.set_concurrent_count(concurrency);

    // NOTE: This loop is time-sensitive. Any long awaits or blocking will throw off measurements
    loop {
        let sample = sampler.sample_tps().await;

        info!(
            "Sample: {}TPS, {}/{} ({}), {}",
            sample.tps(),
            sample.success_count,
            sample.error_count,
            sample.total(),
            format_duration(sample.elapsed),
        );

        if start.elapsed() > config.duration {
            break;
        }
    }
    sampler.wait_for_shutdown().await;

    info!("Scenario complete");
}
