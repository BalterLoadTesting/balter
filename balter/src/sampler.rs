use crate::controllers::{CCOutcome, ConcurrencyController};
use crate::data::{SampleData, SampleSet};
use crate::transaction::{TransactionData, TRANSACTION_HOOK};
use arc_swap::ArcSwap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use metrics_util::AtomicBucket;
use std::future::Future;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
use tokio::time::{interval, Interval};
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

mod base_sampler;
mod concurrency_adjusted_sampler;

const SAMPLE_WINDOW_SIZE: usize = 100;
const SKIP_SIZE: usize = 25;

// TODO: Currently there is some weird idiosyncrasies between the the tps_sampler and the
// ConcurrencyController, namely the fact that they need to have data between them which is kept in
// sync. This is definitely an area for bugs to sneak in, and so a rethink of the data flows would
// be good to remove this tricky area.
pub(crate) struct ConcurrentSampler<T> {
    base_label: String,
    sampler: Sampler<T>,
    cc: ConcurrencyController,
    samples: SampleSet,
    needs_clear: bool,
    tps_limited: bool,
}

impl<T, F> ConcurrentSampler<T>
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    pub(crate) fn new(name: &str, scenario: T, goal_tps: NonZeroU32) -> Self {
        let new = Self {
            base_label: format!("balter_{name}"),
            sampler: Sampler::new(scenario, goal_tps),
            cc: ConcurrencyController::new(goal_tps),
            samples: SampleSet::new(),
            needs_clear: false,
            tps_limited: false,
        };

        if cfg!(feature = "metrics") {
            new.goal_tps_metric(goal_tps);
        }

        new
    }

    pub(crate) async fn get_samples(&mut self) -> (bool, Option<&SampleSet>) {
        // NOTE: We delay clearing our samples to allow for the various controllers to potentially
        // lower the goal_tps. For instance, if we haven't reached our GoalTPS but the error rate
        // is too high, we don't want to adjust the concurrency and clear the data collected -- we
        // want to reset the goal.
        if self.needs_clear {
            trace!("Clearing samples");
            self.samples.clear();
            self.needs_clear = false;
        }

        self.sampler.sample(&mut self.samples).await;

        if self.samples.full() {
            let stable = match self.cc.analyze(&self.samples) {
                CCOutcome::Stable => {
                    if cfg!(feature = "metrics") {
                        // TODO: Given these metric recordings aren't on the hot-path it is likely
                        // okay that we allocate for them. But if there is a simple way to avoid it
                        // that would be preferable.
                        metrics::gauge!(format!("{}_cc_state", &self.base_label)).set(0);
                    }
                    true
                }
                CCOutcome::TpsLimited(max_tps, concurrency) => {
                    // TODO: There is currently no way to get _out_ of being tps_limited. This may
                    // or may not be a problem, but it would be good to evaluate other options.
                    if !self.tps_limited {
                        self.tps_limited = true;
                        warn!("Unable to achieve TPS on current server.");
                    }
                    self.set_concurrency(concurrency);
                    self.set_goal_tps_unchecked(max_tps);

                    if cfg!(feature = "metrics") {
                        metrics::gauge!(format!("{}_cc_state", &self.base_label)).set(-1);
                    }
                    false
                }
                CCOutcome::AlterConcurrency(concurrency) => {
                    self.set_concurrency(concurrency);

                    if cfg!(feature = "metrics") {
                        metrics::gauge!(format!("{}_cc_state", &self.base_label)).set(1);
                    }
                    false
                }
            };

            (stable, Some(&self.samples))
        } else {
            (false, None)
        }
    }

    pub fn goal_tps(&self) -> NonZeroU32 {
        self.sampler.tps_limit
    }

    pub async fn wait_for_shutdown(self) -> OutputStats {
        let stats = OutputStats {
            goal_tps: self.goal_tps(),
            concurrency: self.sampler.concurrency.load(Ordering::Relaxed),
            final_sample_set: self.samples,
            tps_limited: self.tps_limited,
        };
        self.sampler.wait_for_shutdown().await;
        stats
    }

    pub fn set_goal_tps(&mut self, goal_tps: NonZeroU32) {
        if self.tps_limited && goal_tps > self.sampler.tps_limit {
            trace!("Unable to set TPS; TPS is limited");
        } else {
            self.set_goal_tps_unchecked(goal_tps);
        }
    }

    fn set_goal_tps_unchecked(&mut self, goal_tps: NonZeroU32) {
        if goal_tps != self.sampler.tps_limit {
            self.needs_clear = true;
            self.cc.set_goal_tps(goal_tps);
            self.sampler.set_tps_limit(goal_tps);

            if cfg!(feature = "metrics") {
                self.goal_tps_metric(goal_tps);
            }
        }
    }

    fn set_concurrency(&mut self, concurrency: usize) {
        self.needs_clear = true;
        trace!("Setting concurrency to: {concurrency}");
        self.sampler.set_concurrency(concurrency);

        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_concurrency", &self.base_label)).set(concurrency as f64);
        }
    }

    #[cfg(feature = "metrics")]
    fn goal_tps_metric(&self, goal_tps: NonZeroU32) {
        metrics::gauge!(format!("{}_goal_tps", &self.base_label)).set(goal_tps.get());
    }
}

pub(crate) struct OutputStats {
    pub goal_tps: NonZeroU32,
    pub concurrency: usize,
    pub final_sample_set: SampleSet,
    pub tps_limited: bool,
}

pub(crate) struct Sampler<T> {
    scenario: T,
    concurrency: Arc<AtomicUsize>,
    limiter: Arc<ArcSwap<DefaultDirectRateLimiter>>,
    tps_limit: NonZeroU32,

    tasks: Vec<JoinHandle<()>>,
    interval: Interval,
    last_tick: Instant,

    success_count: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
    latency: Arc<AtomicBucket<Duration>>,
}

impl<T, F> Sampler<T>
where
    T: Fn() -> F + Send + Sync + 'static + Clone,
    F: Future<Output = ()> + Send,
{
    pub(crate) fn new(scenario: T, tps_limit: NonZeroU32) -> Self {
        let limiter: DefaultDirectRateLimiter = rate_limiter(tps_limit);
        let limiter: Arc<DefaultDirectRateLimiter> = Arc::new(limiter);
        let limiter: Arc<ArcSwap<DefaultDirectRateLimiter>> = Arc::new(ArcSwap::new(limiter));
        let mut new_self = Self {
            scenario,
            concurrency: Arc::new(AtomicUsize::new(1)),
            limiter,
            tps_limit,

            tasks: vec![],
            interval: interval(Duration::from_millis(200)),
            last_tick: Instant::now(),

            success_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            latency: Arc::new(AtomicBucket::new()),
        };
        new_self.populate_jobs();
        new_self
    }

    // TODO: This function is a bit awkward in taking SampleSet as an argument. Perhaps Sampler
    // should own the SampleSet data-structure.
    pub(crate) async fn sample(&mut self, samples: &mut SampleSet) {
        self.interval.tick().await;

        self.latency.clear_with(|durations| {
            for duration in durations {
                samples.push_latency(*duration);
            }
        });

        let success_count = self.success_count.swap(0, Ordering::Relaxed);
        let error_count = self.error_count.swap(0, Ordering::Relaxed);

        let elapsed = self.last_tick.elapsed();
        self.last_tick = Instant::now();

        let data = SampleData {
            elapsed,
            success: success_count,
            error: error_count,
        };

        // TODO: We should adjust interval timing based on noise not just sample count.
        /*
        if data.total() > 50_000 {
            let new_interval = self.interval.period() / 2;
            self.interval = interval(new_interval);
            // NOTE: First tick() is always instant
            self.interval.tick().await;
        }
        */

        samples.push(data);
    }

    /// NOTE: Panics when concurrent_count=0
    pub(crate) fn set_concurrency(&mut self, concurrency: usize) {
        if concurrency != 0 {
            self.concurrency.store(concurrency, Ordering::Relaxed);
            self.populate_jobs();
        } else {
            panic!("Concurrent count is not allowed to be set to 0.");
        }
    }

    pub(crate) fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        if tps_limit != self.tps_limit {
            self.tps_limit = tps_limit;
            self.limiter.store(Arc::new(rate_limiter(tps_limit)));
        }
    }

    pub(crate) async fn wait_for_shutdown(mut self) {
        self.concurrency.store(0, Ordering::Relaxed);
        self.populate_jobs();
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
    }

    fn populate_jobs(&mut self) {
        let concurrent_count = self.concurrency.load(Ordering::Relaxed);

        if self.tasks.len() > concurrent_count {
            for handle in self.tasks.drain(concurrent_count..) {
                handle.abort();
            }
        } else {
            while self.tasks.len() < concurrent_count {
                let scenario = self.scenario.clone();
                let concurrent_count = self.concurrency.clone();
                let id = self.tasks.len();
                let transaction_data = TransactionData {
                    limiter: self.limiter.clone(),
                    success: self.success_count.clone(),
                    error: self.error_count.clone(),
                    latency: self.latency.clone(),
                };

                self.tasks.push(tokio::spawn(TRANSACTION_HOOK.scope(
                    transaction_data,
                    async move {
                        while id < concurrent_count.load(Ordering::Relaxed) {
                            scenario().await;
                        }
                    },
                )));
            }
        }
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
mod tests {
    use super::*;
    use rand_distr::{Distribution, Normal};

    async fn mock_trivial_scenario() {
        let labels = balter_core::TransactionLabels {
            success: "",
            error: "",
            latency: "",
        };
        let _ = crate::transaction::transaction_hook::<_, (), ()>(labels, async { Ok(()) }).await;
    }

    async fn mock_noisy_scenario() {
        let labels = balter_core::TransactionLabels {
            success: "",
            error: "",
            latency: "",
        };
        let _ = crate::transaction::transaction_hook::<_, (), ()>(labels, async {
            let normal = Normal::new(100., 25.).unwrap();
            let v: f64 = normal.sample(&mut rand::thread_rng());
            tokio::time::sleep(Duration::from_micros(v.floor() as u64)).await;
            Ok(())
        })
        .await;
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore]
    #[ntest::timeout(300)]
    async fn test_simple_case() {
        let mut tps_sampler = Sampler::new(mock_trivial_scenario, NonZeroU32::new(1_000).unwrap());
        tps_sampler.set_concurrency(20);

        let mut samples = SampleSet::new();
        tps_sampler.sample(&mut samples).await;
        for _ in 0..10 {
            tps_sampler.sample(&mut samples).await;
            info!("tps: {}", samples.mean_tps());
            assert!((1_000. - samples.mean_tps()).abs() < 150.);
        }
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore]
    #[ntest::timeout(300)]
    async fn test_noisy_case() {
        let mut tps_sampler = Sampler::new(mock_noisy_scenario, NonZeroU32::new(1_000).unwrap());
        tps_sampler.set_concurrency(20);

        let mut samples = SampleSet::new();
        tps_sampler.sample(&mut samples).await;
        for _ in 0..10 {
            tps_sampler.sample(&mut samples).await;
            info!("tps: {}", samples.mean_tps());
            assert!((1_000. - samples.mean_tps()).abs() < 150.);
        }
    }
}
