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
    time::Duration,
};
use tokio::task::JoinHandle;
use tokio::time::{interval, Instant, Interval};
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

const SKIP_SIZE: usize = 2;

pub(crate) struct BaseSampler<T> {
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
    pub async fn new(scenario: T, tps_limit: NonZeroU32) -> Self {
        Self {
            scenario,
            tasks: vec![],
            timer: Timer::new(balter_core::BASE_INTERVAL).await,
            task_atomics: TaskAtomics::new(tps_limit),
        }
    }

    pub async fn sample(&mut self) -> SampleSet {
        let mut samples = SampleSet::new();
        let mut skip_window = SKIP_SIZE;
        let mut means = vec![];
        loop {
            let elapsed = self.timer.tick().await;
            let provisional = self.task_atomics.collect();
            let per_sample_count = provisional.count();

            if per_sample_count < balter_core::MIN_SAMPLE_COUNT {
                trace!("Not enough sample count. Found {per_sample_count}. Doubling.");
                self.timer.double().await;

                // A tiny optimization to speed up sampling particularly in low-TPS or
                // high-latency situations.
                if self.concurrency() < 50 {
                    self.set_concurrency(self.concurrency() * 2);
                }

                continue;
            }

            // NOTE: We skip the first N mainly because they have tended to be the noisiest.
            if skip_window > 0 {
                skip_window -= 1;
                trace!("Within sample skip window.");
                continue;
            }

            samples.push(SampleData {
                success: provisional.success,
                error: provisional.error,
                elapsed,
            });
            samples.push_latencies(provisional.latency);

            if samples.len() > 10 {
                trace!("Enough samples collected.");
                // NOTE: Could use a statistical review here.
                means.push(samples.mean_tps());
                if is_stable(&means, 5) {
                    if samples.len() < 25 && per_sample_count > balter_core::ADJUSTABLE_SAMPLE_COUNT
                    {
                        trace!("Halving timer");
                        self.timer.halve().await;
                    }

                    return samples;
                } else if samples.len() > 100 {
                    error!("Balter is unable to find a stable measurement.");
                    samples.clear();
                } else {
                    trace!("Waiting on stabilization.");
                    trace!("Mean measurements: {means:?}");
                }
            }
        }
    }

    pub fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        self.task_atomics.set_tps_limit(tps_limit);
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.task_atomics.tps_limit
    }

    pub fn set_concurrency(&mut self, concurrency: usize) {
        if self.tasks.len() == concurrency {
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

    pub async fn shutdown(mut self) {
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

    async fn double(&mut self) {
        if self.interval_dur < Duration::from_secs(10) {
            self.interval_dur *= 2;
            *self = Self::new(self.interval_dur).await;
        } else {
            error!("Balter's Sampling interval is greater than 10s. This is likely a sign of an issue; not increasing the sampling interval.")
        }
    }

    async fn halve(&mut self) {
        self.interval_dur *= 2;
        *self = Self::new(self.interval_dur).await;
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

    fn collect(&self) -> ProvisionalData {
        let success = self.success.swap(0, Ordering::Relaxed);
        let error = self.error.swap(0, Ordering::Relaxed);
        let mut latency = vec![];
        self.latency.clear_with(|dur| {
            latency.extend_from_slice(dur);
        });

        ProvisionalData {
            success,
            error,
            latency,
        }
    }
}

struct ProvisionalData {
    success: u64,
    error: u64,
    latency: Vec<Duration>,
}

impl ProvisionalData {
    fn count(&self) -> u64 {
        self.success + self.error
    }
}

fn rate_limiter(tps_limit: NonZeroU32) -> DefaultDirectRateLimiter {
    RateLimiter::direct(
        Quota::per_second(tps_limit)
            // TODO: Make burst configurable
            .allow_burst(NonZeroU32::new(1).unwrap()),
    )
}

fn is_stable(values: &[f64], count: usize) -> bool {
    let diffs: Vec<_> = values
        .windows(2)
        .map(|arr| {
            // % difference
            (arr[1] - arr[0]) / arr[0]
        })
        .collect();

    diffs.iter().rev().take_while(|x| **x < 0.02).count() >= count - 1
}

fn is_decreasing(values: &[f64], count: usize) -> bool {
    values
        .windows(2)
        .rev()
        .take(count - 1)
        .map(|arr| arr[1] < arr[0])
        .all(|x| x)
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
                let mean: Duration = $m;
                let std: Duration = $s;
                let _ = crate::transaction::transaction_hook::<_, (), ()>(labels, async {
                    let normal =
                        SkewNormal::new(mean.as_secs_f64(), std.as_secs_f64(), 20.).unwrap();
                    let v: f64 = normal.sample(&mut rand::thread_rng()).max(0.);
                    tokio::time::sleep(Duration::from_secs_f64(v)).await;
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
            mock_scenario!(Duration::from_millis(1), Duration::from_millis(0)),
            NonZeroU32::new(1_000).unwrap(),
        )
        .await;

        sampler.set_concurrency(11);

        let samples = sampler.sample().await;
        assert!(samples.mean_tps() >= 990. && samples.mean_tps() <= 1_010.);
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_noisy() {
        let mut sampler = BaseSampler::new(
            mock_scenario!(Duration::from_millis(10), Duration::from_millis(5)),
            NonZeroU32::new(10_000).unwrap(),
        )
        .await;

        sampler.set_concurrency(210);

        let samples = sampler.sample().await;
        assert!(samples.mean_tps() >= 9_000. && samples.mean_tps() <= 10_010.);
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_slow() {
        let mut sampler = BaseSampler::new(
            mock_scenario!(Duration::from_millis(400), Duration::from_millis(100)),
            NonZeroU32::new(50).unwrap(),
        )
        .await;

        sampler.set_concurrency(100);

        let samples = sampler.sample().await;
        assert!(samples.mean_tps() >= 49. && samples.mean_tps() <= 51.);
    }

    #[test]
    fn test_stability_simple() {
        let arr = [5., 6., 10., 10., 10.];
        assert!(is_stable(&arr, 3));
        assert!(!is_stable(&arr, 4));
    }

    #[test]
    fn test_stability_close_values() {
        let arr = [9., 9., 9.8, 9.9, 10.];
        assert!(is_stable(&arr, 3));
        assert!(!is_stable(&arr, 4));
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_is_decreasing_true() {
        let arr = [10., 5., 8., 7., 6.];
        assert!(is_decreasing(&arr, 3));
        assert!(!is_decreasing(&arr, 4));
    }

    #[test]
    fn test_is_decreasing_false() {
        let arr = [10., 11., 8., 7., 9.];
        assert!(!is_decreasing(&arr, 2));
        assert!(!is_decreasing(&arr, 4));
    }
}
