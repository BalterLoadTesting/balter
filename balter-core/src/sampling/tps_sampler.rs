use crate::transaction::{TransactionData, TRANSACTION_HOOK};
use arc_swap::ArcSwap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
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
use tracing::{debug, error, info, trace};

#[derive(Debug, Copy, Clone)]
pub(crate) struct TpsData {
    pub success_count: u64,
    pub error_count: u64,
    pub elapsed: Duration,
}

impl TpsData {
    #[allow(unused)]
    pub fn new() -> Self {
        Self {
            success_count: 0,
            error_count: 0,
            elapsed: Duration::new(0, 0),
        }
    }

    pub fn tps(&self) -> f64 {
        self.total() as f64 / self.elapsed.as_nanos() as f64 * 1e9
    }

    pub fn error_rate(&self) -> f64 {
        self.error_count as f64 / self.total() as f64
    }

    pub fn total(&self) -> u64 {
        self.success_count + self.error_count
    }
}

pub(crate) struct TpsSampler<T> {
    scenario: T,
    concurrent_count: Arc<AtomicUsize>,
    limiter: Arc<ArcSwap<DefaultDirectRateLimiter>>,
    tps_limit: NonZeroU32,

    tasks: Vec<JoinHandle<()>>,
    interval: Interval,
    last_tick: Instant,

    success_count: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
}

impl<T, F> TpsSampler<T>
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
            concurrent_count: Arc::new(AtomicUsize::new(1)),
            limiter,
            tps_limit,

            tasks: vec![],
            interval: interval(Duration::from_millis(30)),
            last_tick: Instant::now(),

            success_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
        };
        new_self.populate_jobs();
        new_self
    }

    pub(crate) async fn sample_tps(&mut self) -> TpsData {
        self.interval.tick().await;
        let success_count = self.success_count.swap(0, Ordering::Relaxed);
        let error_count = self.error_count.swap(0, Ordering::Relaxed);
        let elapsed = self.last_tick.elapsed();
        self.last_tick = Instant::now();
        TpsData {
            elapsed,
            success_count,
            error_count,
        }
    }

    /// NOTE: Panics when concurrent_count=0
    pub(crate) fn set_concurrent_count(&mut self, concurrent_count: usize) {
        if concurrent_count != 0 {
            self.concurrent_count
                .store(concurrent_count, Ordering::Relaxed);
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
        self.concurrent_count.store(0, Ordering::Relaxed);
        for task in self.tasks.drain(..) {
            // TODO: Timeout in case a scenario loops indefinitely
            task.await.expect("Task unexpectedly failed.");
        }
    }

    fn populate_jobs(&mut self) {
        let concurrent_count = self.concurrent_count.load(Ordering::Relaxed);

        if self.tasks.len() > concurrent_count {
            // TODO: Clean up the tasks cleanly + timeout/abort in case a scenario loops
            // indefinitely
            self.tasks.truncate(concurrent_count);
        } else {
            while self.tasks.len() < concurrent_count {
                let scenario = self.scenario.clone();
                let concurrent_count = self.concurrent_count.clone();
                let id = self.tasks.len();
                let transaction_data = TransactionData {
                    // TODO: Use ArcSwap here
                    limiter: self.limiter.clone(),
                    success: self.success_count.clone(),
                    error: self.error_count.clone(),
                };

                trace!("Spawning a new task with id {id}.");
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
        let _ = crate::transaction::transaction_hook::<_, (), ()>(async { Ok(()) }).await;
    }

    async fn mock_noisy_scenario() {
        let _ = crate::transaction::transaction_hook::<_, (), ()>(async {
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
        let mut tps_sampler =
            TpsSampler::new(mock_trivial_scenario, NonZeroU32::new(1_000).unwrap());
        tps_sampler.set_concurrent_count(20);

        let _sample = tps_sampler.sample_tps().await;
        for _ in 0..10 {
            let sample = tps_sampler.sample_tps().await;
            info!("tps: {}", sample.tps());
            assert!((1_000. - sample.tps()).abs() < 150.);
        }
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore]
    #[ntest::timeout(300)]
    async fn test_noisy_case() {
        let mut tps_sampler = TpsSampler::new(mock_noisy_scenario, NonZeroU32::new(1_000).unwrap());
        tps_sampler.set_concurrent_count(20);

        let _sample = tps_sampler.sample_tps().await;
        for _ in 0..10 {
            let sample = tps_sampler.sample_tps().await;
            info!("tps: {}", sample.tps());
            assert!((1_000. - sample.tps()).abs() < 150.);
        }
    }
}
