use crate::{
    scenario::BoxedFut,
    transaction::{TransactionData, TRANSACTION_HOOK},
};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::task::JoinSet;
use tracing::{debug, trace, warn};

// TODO: Experimentally find values rather than just guessing.
const DEFAULT_SAMPLE_SIZE: u64 = 20;

pub(crate) struct TpsSampler {
    scenario: fn() -> BoxedFut,
    pub(crate) limiter: Option<Arc<DefaultDirectRateLimiter>>,
    success: Arc<AtomicU64>,
    error: Arc<AtomicU64>,
    pub(crate) concurrent_count: usize,
    pub(crate) batch_size: usize,
    join_set: JoinSet<()>,
    timer: Instant,
    tps_limit: f64,
}

impl TpsSampler {
    pub(crate) fn new(scenario: fn() -> BoxedFut, goal_tps: f64) -> Self {
        let mut new_self = Self {
            scenario,
            limiter: None,
            tps_limit: 0.,
            success: Arc::new(AtomicU64::new(0)),
            error: Arc::new(AtomicU64::new(0)),
            concurrent_count: 1,
            batch_size: 1,
            join_set: JoinSet::new(),
            timer: Instant::now(),
        };
        new_self.set_tps_limit(goal_tps);
        new_self
    }

    pub(crate) async fn sample_tps(&mut self) -> Option<TpsData> {
        let mut data = SampleData::new();
        let mut res = None;

        while res.is_none() {
            #[allow(clippy::redundant_pattern_matching)]
            if let Some(_) = self.join_set.join_next().await {
                // 1. On each finished scenario batch we check measurements.
                let sample_data = self.sample_data();

                // 2. Adjust our batch-size; ideally every time we return a batch of scenarios we
                //    have enough data to make adjustments.
                if sample_data.total() == 0 {
                    // NOTE: Sometimes a scenario will finish without having any data; this is
                    // because the scenario is bad (doesn't call any transactions) or because two
                    // scenarios finish at nearly the same time.
                    warn!("No transaction data for completed scenario.");
                } else if sample_data.total() < DEFAULT_SAMPLE_SIZE {
                    // TODO: Really simplistic adaptive batch controls here for scenarios. Would be
                    // good to have something smarter, but this works and doesn't blow up.
                    self.batch_size += 1;
                    trace!("Increasing sampling batch size to {}", self.batch_size);
                } else if sample_data.total() > 2 * DEFAULT_SAMPLE_SIZE {
                    self.batch_size -= 1;
                    trace!("Reducing sampling batch size to {}", self.batch_size);
                }

                // 3. Increment our sample counts.
                data += sample_data;

                // 4. Once we have enough data, we can make our timing measurement.
                if data.total() >= DEFAULT_SAMPLE_SIZE {
                    let elapsed = self.timer.elapsed();
                    self.timer = Instant::now();

                    res = Some(data.tps_data(elapsed));
                }
            }

            // Repopulate scenario runs
            self.populate_jobs();
        }

        res
    }

    fn populate_jobs(&mut self) {
        while self.join_set.len() < self.concurrent_count {
            let scenario = self.scenario;
            let transaction_data = TransactionData {
                limiter: self.limiter.clone(),
                success: self.success.clone(),
                error: self.error.clone(),
            };
            let batch_size = self.batch_size;
            self.join_set
                .spawn(TRANSACTION_HOOK.scope(transaction_data, async move {
                    let _timer = Instant::now();
                    for _ in 0..batch_size {
                        scenario().await
                    }
                }));
        }
    }

    fn sample_data(&self) -> SampleData {
        let success_count = self.success.fetch_min(0, Ordering::Relaxed);
        let error_count = self.error.fetch_min(0, Ordering::Relaxed);

        SampleData {
            success_count,
            error_count,
        }
    }

    pub(crate) fn set_concurrent_count(&mut self, concurrent_count: usize) {
        self.concurrent_count = concurrent_count;
        self.populate_jobs();
    }

    pub(crate) fn set_tps_limit(&mut self, tps: f64) {
        if (self.tps_limit - tps).abs() > f64::EPSILON {
            debug!("New rate limiter set for {tps} TPS.");
            let limiter = RateLimiter::direct(Quota::per_second(
                NonZeroU32::new(tps.floor() as u32).unwrap(),
            ));
            self.limiter = Some(Arc::new(limiter));
            self.tps_limit = tps;
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct TpsData {
    pub success_count: u64,
    pub error_count: u64,
    pub elapsed: Duration,
}

impl TpsData {
    pub(crate) fn tps(&self) -> f64 {
        (self.success_count + self.error_count) as f64 / self.elapsed.as_millis() as f64 * 1000.
    }

    pub(crate) fn error_rate(&self) -> f64 {
        self.error_count as f64 / (self.success_count + self.error_count) as f64
    }
}

#[derive(Debug, Copy, Clone)]
struct SampleData {
    success_count: u64,
    error_count: u64,
}

impl SampleData {
    fn new() -> Self {
        Self {
            success_count: 0,
            error_count: 0,
        }
    }

    fn total(&self) -> u64 {
        self.success_count + self.error_count
    }

    fn tps_data(&self, elapsed: Duration) -> TpsData {
        TpsData {
            success_count: self.success_count,
            error_count: self.error_count,
            elapsed,
        }
    }
}

impl std::ops::Add for SampleData {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            success_count: self.success_count + rhs.success_count,
            error_count: self.error_count + rhs.error_count,
        }
    }
}

impl std::ops::AddAssign for SampleData {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_scenario() -> BoxedFut {
        Box::pin(async move {
            let _ = crate::transaction::transaction_hook::<_, (), ()>(async { Ok(()) }).await;
        })
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_tps_sampler() {
        let mut tps_sampler = TpsSampler::new(mock_scenario, 0.);

        let _tps_data = tps_sampler.sample_tps().await;

        assert!(tps_sampler.batch_size > 1);
        let _tps_data = tps_sampler.sample_tps().await;
        let _tps_data = tps_sampler.sample_tps().await;
        let _tps_data = tps_sampler.sample_tps().await;
        let _tps_data = tps_sampler.sample_tps().await;
        let _tps_data = tps_sampler.sample_tps().await;
        let _tps_data = tps_sampler.sample_tps().await;
        let _tps_data = tps_sampler.sample_tps().await;
        let prev_size = tps_sampler.batch_size;
        let _tps_data = tps_sampler.sample_tps().await;
        assert_eq!(tps_sampler.batch_size, prev_size);
    }
}
