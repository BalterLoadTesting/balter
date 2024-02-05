use crate::{
    scenario::{batch_size_controller::BatchSizeController, BoxedFut},
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
#[allow(unused)]
use tracing::{debug, error, trace};

pub(crate) struct TpsSampler {
    scenario: fn() -> BoxedFut,
    pub(crate) limiter: Option<Arc<DefaultDirectRateLimiter>>,
    pub(crate) concurrent_count: u64,
    join_set: JoinSet<(CorrelationData, TpsData)>,
    tps_limit: f64,
    batch_controller: BatchSizeController,
}

impl TpsSampler {
    pub(crate) async fn new(scenario: fn() -> BoxedFut, goal_tps: f64) -> Self {
        let mut new_self = Self {
            scenario,
            limiter: None,
            tps_limit: 0.,
            concurrent_count: 1,
            join_set: JoinSet::new(),
            batch_controller: BatchSizeController::new().await,
        };
        new_self.set_tps_limit(goal_tps);
        new_self.populate_jobs();
        new_self
    }

    pub(crate) async fn sample_tps(&mut self) -> Option<TpsData> {
        let mut res;
        loop {
            let mut bad_data = false;

            res = if let Some(tps_data) = self.join_set.join_next().await {
                // TODO: Properly handle errors
                let (cor_data, tps_data) = tps_data.unwrap();

                if cor_data.concurrent_count != self.concurrent_count
                    || cor_data.batch_size != self.batch_controller.batch_size()
                {
                    bad_data = true;
                } else if tps_data.total() == 0 {
                    error!("No transaction data for completed scenario.");
                    return None;
                } else {
                    self.batch_controller.push(tps_data.elapsed);
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
            let batch_size = self.batch_controller.batch_size();
            let concurrent_count = self.concurrent_count;
            self.join_set
                .spawn(TRANSACTION_HOOK.scope(transaction_data, async move {
                    let timer = Instant::now();
                    for _ in 0..batch_size {
                        scenario().await
                    }

                    let elapsed = timer.elapsed();
                    let success_count = success.fetch_min(0, Ordering::Relaxed);
                    let error_count = error.fetch_min(0, Ordering::Relaxed);
                    (
                        CorrelationData {
                            concurrent_count,
                            batch_size,
                        },
                        TpsData {
                            success_count,
                            error_count,
                            elapsed,
                        },
                    )
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

struct CorrelationData {
    concurrent_count: u64,
    batch_size: u64,
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
        let mut tps_sampler = TpsSampler::new(mock_scenario, 0.).await;

        for _ in 0..30 {
            let _tps_data = tps_sampler.sample_tps().await;
        }

        let prev_size = tps_sampler.batch_controller.batch_size();
        let _tps_data = tps_sampler.sample_tps().await;
        assert_eq!(tps_sampler.batch_controller.batch_size(), prev_size);
    }
}
