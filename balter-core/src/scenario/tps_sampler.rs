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
use tracing::{debug, error, trace};

// TODO: The value here should be to compensate for the overhead of JoinSet, which is (maybe) a
// constant time factor. Not entirely clear how to dynamically size this value to minimize JoinSet
// overhead while maximizing responsiveness.
const DEFAULT_SAMPLE_SIZE: u64 = 200;

pub(crate) struct TpsSampler {
    scenario: fn() -> BoxedFut,
    pub(crate) limiter: Option<Arc<DefaultDirectRateLimiter>>,
    pub(crate) concurrent_count: u64,
    pub(crate) batch_size: u64,
    join_set: JoinSet<TpsData>,
    tps_limit: f64,
}

impl TpsSampler {
    pub(crate) fn new(scenario: fn() -> BoxedFut, goal_tps: f64) -> Self {
        let mut new_self = Self {
            scenario,
            limiter: None,
            tps_limit: 0.,
            concurrent_count: 1,
            batch_size: 1,
            join_set: JoinSet::new(),
        };
        new_self.set_tps_limit(goal_tps);
        new_self.populate_jobs();
        new_self
    }

    pub(crate) async fn sample_tps(&mut self) -> Option<TpsData> {
        let mut res;
        loop {
            // At the start we might collect too little data to have any valid measurements.
            // Effectively this is a warmup period without being explicit about it. Would be nice
            // to make this adaptive somehow.
            let mut bad_data = false;

            //#[allow(clippy::redundant_pattern_matching)]
            res = if let Some(tps_data) = self.join_set.join_next().await {
                // TODO: Properly handle errors
                let tps_data = tps_data.unwrap();

                if tps_data.total() == 0 {
                    error!("No transaction data for completed scenario.");
                    return None;
                } else if tps_data.total() < DEFAULT_SAMPLE_SIZE {
                    let transactions_per_run = tps_data.total() / self.batch_size;
                    let optimal_batch_size = DEFAULT_SAMPLE_SIZE / transactions_per_run;
                    self.batch_size = optimal_batch_size;
                    bad_data = true;
                    debug!("Increasing sampling batch size to {}", self.batch_size);
                } else if tps_data.total() > 2 * DEFAULT_SAMPLE_SIZE {
                    // TODO: Really simplistic adaptive batch controls here for scenarios. Would be
                    // good to have something smarter, but this works and doesn't blow up.
                    self.batch_size -= 1;
                    trace!("Reducing sampling batch size to {}", self.batch_size);
                }

                // TODO: Sharsty stats. Since each returned value is 1/N of the number of tasks, we
                // just multiply here. But it means our success and error counts in the TpsData are
                // fudged.
                let mut tps_data = tps_data;
                tps_data.success_count *= self.concurrent_count;
                tps_data.error_count *= self.concurrent_count;

                Some(tps_data)
            } else {
                error!("Something went wrong");
                return None;
            };

            self.populate_jobs();

            if !bad_data {
                break;
            }
        }

        res
    }

    fn populate_jobs(&mut self) {
        while self.join_set.len() < self.concurrent_count as usize {
            let scenario = self.scenario;

            let success = Arc::new(AtomicU64::new(0));
            let error = Arc::new(AtomicU64::new(0));
            let transaction_data = TransactionData {
                limiter: self.limiter.clone(),
                success: success.clone(),
                error: error.clone(),
            };
            let batch_size = self.batch_size;
            self.join_set
                .spawn(TRANSACTION_HOOK.scope(transaction_data, async move {
                    let timer = Instant::now();
                    for _ in 0..batch_size {
                        scenario().await
                    }

                    let elapsed = timer.elapsed();
                    let success_count = success.fetch_min(0, Ordering::Relaxed);
                    let error_count = error.fetch_min(0, Ordering::Relaxed);
                    TpsData {
                        success_count,
                        error_count,
                        elapsed,
                    }
                }));
        }
    }

    pub(crate) fn set_concurrent_count(&mut self, concurrent_count: u64) {
        self.concurrent_count = concurrent_count;
        self.populate_jobs();
    }

    pub(crate) fn set_tps_limit(&mut self, tps: f64) {
        if (self.tps_limit - tps).abs() > f64::EPSILON {
            debug!("New rate limiter set for {tps} TPS.");
            let limiter = RateLimiter::direct(
                Quota::per_second(NonZeroU32::new(tps.floor() as u32).unwrap())
                    .allow_burst(NonZeroU32::new(100).unwrap()),
            );
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
        let prev_size = tps_sampler.batch_size;
        let _tps_data = tps_sampler.sample_tps().await;
        assert_eq!(tps_sampler.batch_size, prev_size);
    }
}
