use arc_swap::ArcSwap;
use governor::DefaultDirectRateLimiter;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Transaction hook used by the `#[transaction]` macro. Not intended to be used manually.
pub async fn transaction_hook<T, R, E>(func: T) -> T::Output
where
    T: Future<Output = Result<R, E>>,
{
    // TODO: Remove clone
    if let Ok(hook) = TRANSACTION_HOOK.try_with(|v| v.clone()) {
        {
            let limiter = hook.limiter.load();
            limiter.until_ready().await;
        }

        let res = func.await;

        if res.is_ok() {
            hook.success.fetch_add(1, Ordering::Relaxed);
        } else {
            hook.error.fetch_add(1, Ordering::Relaxed);
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
