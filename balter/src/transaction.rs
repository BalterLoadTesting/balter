use arc_swap::ArcSwap;
use balter_core::TransactionLabels;
use governor::DefaultDirectRateLimiter;
use std::time::Instant;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Transaction hook used by the `#[transaction]` macro. Not intended to be used manually.
pub async fn transaction_hook<T, R, E>(labels: TransactionLabels, func: T) -> T::Output
where
    T: Future<Output = Result<R, E>>,
{
    // TODO: Remove clone
    if let Ok(hook) = TRANSACTION_HOOK.try_with(|v| v.clone()) {
        {
            let limiter = hook.limiter.load();
            limiter.until_ready().await;
        }

        let start = Instant::now();
        let res = func.await;
        let elapsed = start.elapsed();

        if cfg!(feature = "metrics") {
            // TODO: What are the implications of calling this every time?
            metrics::describe_histogram!(labels.latency, metrics::Unit::Nanoseconds, "");
            metrics::histogram!(labels.latency).record(elapsed.as_nanos() as f64);
        }

        if res.is_ok() {
            hook.success.fetch_add(1, Ordering::Relaxed);

            if cfg!(feature = "metrics") {
                metrics::counter!(labels.success).increment(1);
            }
        } else {
            hook.error.fetch_add(1, Ordering::Relaxed);
            if cfg!(feature = "metrics") {
                metrics::counter!(labels.error).increment(1);
            }
        }

        res
    } else {
        tracing::error!("No hook available.");
        func.await
    }
}

#[derive(Clone)]
pub(crate) struct TransactionData {
    pub limiter: Arc<ArcSwap<DefaultDirectRateLimiter>>,
    pub success: Arc<AtomicU64>,
    pub error: Arc<AtomicU64>,
}

tokio::task_local! {
    pub(crate) static TRANSACTION_HOOK: TransactionData;
}
