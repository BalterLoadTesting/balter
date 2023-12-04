use governor::DefaultDirectRateLimiter;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Transaction hook used by the `#[transaction]` macro. Not intended to be used manually.
pub async fn transaction_hook<T: Future<Output = Result<R, E>>, R, E>(func: T) -> T::Output {
    if let Ok(hook) = TRANSACTION_HOOK.try_with(|v| v.clone()) {
        if let Some(limiter) = &hook.limiter {
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
        tracing::warn!("No hook available.");
        func.await
    }
}

#[derive(Clone)]
pub(crate) struct TransactionData {
    pub limiter: Option<Arc<DefaultDirectRateLimiter>>,
    pub success: Arc<AtomicU64>,
    pub error: Arc<AtomicU64>,
}

tokio::task_local! {
    pub(crate) static TRANSACTION_HOOK: TransactionData;
}
