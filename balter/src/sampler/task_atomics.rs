use crate::measurement::Measurement;
use crate::transaction::TransactionData;
use arc_swap::ArcSwap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use metrics_util::AtomicBucket;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct TaskAtomics {
    limiter: Arc<ArcSwap<DefaultDirectRateLimiter>>,
    tps_limit: NonZeroU32,
    success: Arc<AtomicU64>,
    error: Arc<AtomicU64>,
    latency: Arc<AtomicBucket<Duration>>,
}

impl TaskAtomics {
    pub fn new(tps_limit: NonZeroU32) -> Self {
        Self {
            limiter: Arc::new(ArcSwap::new(Arc::new(rate_limiter(tps_limit)))),
            tps_limit,
            success: Arc::new(AtomicU64::new(0)),
            error: Arc::new(AtomicU64::new(0)),
            latency: Arc::new(AtomicBucket::new()),
        }
    }

    pub fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        if tps_limit != self.tps_limit {
            self.tps_limit = tps_limit;
            self.limiter.store(Arc::new(rate_limiter(tps_limit)));
        }
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.tps_limit
    }

    pub fn clone_to_transaction_data(&self) -> TransactionData {
        TransactionData {
            limiter: self.limiter.clone(),
            success: self.success.clone(),
            error: self.error.clone(),
            latency: self.latency.clone(),
        }
    }

    pub fn collect(&self, elapsed: Duration) -> Measurement {
        let success = self.success.swap(0, Ordering::Relaxed);
        let error = self.error.swap(0, Ordering::Relaxed);
        let mut measurements = Measurement::new(success, error, elapsed);
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
