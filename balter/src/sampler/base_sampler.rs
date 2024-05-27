use crate::measurements::Measurements;
use crate::transaction::{TransactionData, TRANSACTION_HOOK};
use arc_swap::ArcSwap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use metrics_util::AtomicBucket;
use std::future::Future;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::task::JoinHandle;
use tokio::time::{interval, Instant, Interval};
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

pub(crate) struct BaseSampler<T> {
    base_label: String,
    scenario: T,
    tasks: Vec<JoinHandle<()>>,
    timer: Timer,
    task_atomics: TaskAtomics,
}

impl<T, F> BaseSampler<T>
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    pub async fn new(name: &str, scenario: T, tps_limit: NonZeroU32) -> Self {
        Self {
            base_label: format!("balter_{name}"),
            scenario,
            tasks: vec![],
            timer: Timer::new(balter_core::BASE_INTERVAL).await,
            task_atomics: TaskAtomics::new(tps_limit),
        }
    }

    pub async fn sample(&mut self) -> Measurements {
        let elapsed = self.timer.tick().await;
        let measurements = self.task_atomics.collect(elapsed);

        let latency_p50 = measurements.latency(0.5);
        if latency_p50 > self.timer.interval_dur {
            self.timer.set_interval_dur(latency_p50 * 2).await;
        }

        trace!("{measurements}");

        measurements
    }

    pub fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_goal_tps", &self.base_label)).set(tps_limit.get());
        }

        self.task_atomics.set_tps_limit(tps_limit);
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.task_atomics.tps_limit
    }

    pub fn set_concurrency(&mut self, concurrency: usize) {
        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_concurrency", &self.base_label)).set(concurrency as f64);
        }

        #[allow(clippy::comparison_chain)]
        if self.tasks.len() == concurrency {
            #[allow(clippy::needless_return)]
            return;
        } else if self.tasks.len() > concurrency {
            for handle in self.tasks.drain(concurrency..) {
                handle.abort();
            }
        } else {
            while self.tasks.len() < concurrency {
                let scenario = self.scenario.clone();
                let transaction_data = self.task_atomics.clone_to_transaction_data();

                self.tasks.push(tokio::spawn(TRANSACTION_HOOK.scope(
                    transaction_data,
                    async move {
                        // NOTE: We have an outer loop just in case the user-provided
                        // scenario does not have a loop.
                        loop {
                            scenario().await;
                        }
                    },
                )));
            }
        }
    }

    pub fn concurrency(&self) -> usize {
        self.tasks.len()
    }

    pub fn shutdown(mut self) {
        self.set_concurrency(0);
    }
}

struct Timer {
    interval: Interval,
    last_tick: Instant,
    interval_dur: Duration,
}

impl Timer {
    async fn new(interval_dur: Duration) -> Self {
        let mut interval = interval(interval_dur);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // NOTE: First tick completes instantly
        let last_tick = interval.tick().await;
        Self {
            interval,
            last_tick,
            interval_dur,
        }
    }

    async fn tick(&mut self) -> Duration {
        let next = self.interval.tick().await;
        let elapsed = self.last_tick.elapsed();
        self.last_tick = next;
        elapsed
    }

    async fn set_interval_dur(&mut self, dur: Duration) {
        if dur < Duration::from_secs(10) {
            *self = Self::new(dur).await;
        } else {
            error!("Balter's polling interval is greater than 10s. This is likely a sign of an issue; not increasing the polling interval.")
        }
    }

    #[allow(unused)]
    async fn double(&mut self) {
        if self.interval_dur < Duration::from_secs(10) {
            self.interval_dur *= 2;
            *self = Self::new(self.interval_dur).await;
        } else {
            error!("Balter's Sampling interval is greater than 10s. This is likely a sign of an issue; not increasing the sampling interval.")
        }
    }

    #[allow(unused)]
    async fn halve(&mut self) {
        self.interval_dur *= 2;
        *self = Self::new(self.interval_dur).await;
    }
}

impl std::fmt::Display for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", humantime::format_duration(self.interval_dur))
    }
}

struct TaskAtomics {
    limiter: Arc<ArcSwap<DefaultDirectRateLimiter>>,
    tps_limit: NonZeroU32,
    success: Arc<AtomicU64>,
    error: Arc<AtomicU64>,
    latency: Arc<AtomicBucket<Duration>>,
}

impl TaskAtomics {
    fn new(tps_limit: NonZeroU32) -> Self {
        Self {
            limiter: Arc::new(ArcSwap::new(Arc::new(rate_limiter(tps_limit)))),
            tps_limit,
            success: Arc::new(AtomicU64::new(0)),
            error: Arc::new(AtomicU64::new(0)),
            latency: Arc::new(AtomicBucket::new()),
        }
    }

    fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        if tps_limit != self.tps_limit {
            self.tps_limit = tps_limit;
            self.limiter.store(Arc::new(rate_limiter(tps_limit)));
        }
    }

    fn clone_to_transaction_data(&self) -> TransactionData {
        TransactionData {
            limiter: self.limiter.clone(),
            success: self.success.clone(),
            error: self.error.clone(),
            latency: self.latency.clone(),
        }
    }

    fn collect(&self, elapsed: Duration) -> Measurements {
        let success = self.success.swap(0, Ordering::Relaxed);
        let error = self.error.swap(0, Ordering::Relaxed);
        let mut measurements = Measurements::new(success, error, elapsed);
        self.latency
            .clear_with(|dur| measurements.populate_latencies(dur));
        measurements
    }
}

fn rate_limiter(tps_limit: NonZeroU32) -> DefaultDirectRateLimiter {
    RateLimiter::direct(
        Quota::per_second(tps_limit)
            // TODO: Make burst configurable
            .allow_burst(NonZeroU32::new(1).unwrap()),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rand_distr::{Distribution, SkewNormal};

    #[macro_export]
    macro_rules! mock_scenario {
        ($m:expr, $s:expr) => {
            || async {
                let labels = balter_core::TransactionLabels {
                    success: "",
                    error: "",
                    latency: "",
                };
                let mean: std::time::Duration = $m;
                let std: std::time::Duration = $s;
                let _ = $crate::transaction::transaction_hook::<_, (), ()>(labels, async {
                    let normal =
                        SkewNormal::new(mean.as_secs_f64(), std.as_secs_f64(), 20.).unwrap();
                    let v: f64 = normal.sample(&mut rand::thread_rng()).max(0.);
                    tokio::time::sleep(std::time::Duration::from_secs_f64(v)).await;
                    Ok(())
                })
                .await;
            }
        };
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_simple() {
        let mut sampler = BaseSampler::new(
            "",
            mock_scenario!(Duration::from_millis(1), Duration::from_micros(10)),
            NonZeroU32::new(1_000).unwrap(),
        )
        .await;

        sampler.set_concurrency(11);

        let sample = sampler.sample().await;
        assert!(sample.tps >= 990. && sample.tps <= 1_010.);
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_noisy() {
        let mut sampler = BaseSampler::new(
            "",
            mock_scenario!(Duration::from_millis(10), Duration::from_millis(5)),
            NonZeroU32::new(1_000).unwrap(),
        )
        .await;

        sampler.set_concurrency(210);

        let sample = sampler.sample().await;
        assert!(sample.tps >= 900. && sample.tps <= 1100.);
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_slow() {
        let mut sampler = BaseSampler::new(
            "",
            mock_scenario!(Duration::from_millis(400), Duration::from_millis(100)),
            NonZeroU32::new(50).unwrap(),
        )
        .await;

        sampler.set_concurrency(100);

        let _ = sampler.sample().await;
        let sample = sampler.sample().await;
        dbg!(&sample);
        assert!(sample.tps >= 46. && sample.tps <= 51.);
    }
}
