use super::task_atomics::TaskAtomics;
use super::timer::Timer;
use crate::measurement::Measurement;
use crate::transaction::TRANSACTION_HOOK;
use std::future::Future;
use std::num::NonZeroU32;
use tokio::task::JoinHandle;
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

pub(crate) struct BaseSampler<T> {
    base_label: String,
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
    pub async fn new(name: &str, scenario: T, tps_limit: NonZeroU32) -> Self {
        let interval = if tps_limit.get() < 150 {
            balter_core::BASE_INTERVAL_SLOW
        } else {
            balter_core::BASE_INTERVAL
        };
        let timer = Timer::new(interval).await;
        Self {
            base_label: format!("balter_{name}"),
            scenario,
            tasks: vec![],
            timer,
            task_atomics: TaskAtomics::new(tps_limit),
        }
    }

    pub async fn sample(&mut self) -> Measurement {
        let elapsed = self.timer.tick().await;
        let measurements = self.task_atomics.collect(elapsed);
        trace!("{measurements}");
        measurements
    }

    pub fn set_tps_limit(&mut self, tps_limit: NonZeroU32) {
        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_goal_tps", &self.base_label)).set(tps_limit.get());
        }

        self.task_atomics.set_tps_limit(tps_limit);
    }

    pub fn tps_limit(&self) -> NonZeroU32 {
        self.task_atomics.tps_limit()
    }

    pub fn set_concurrency(&mut self, concurrency: usize) {
        if cfg!(feature = "metrics") {
            metrics::gauge!(format!("{}_concurrency", &self.base_label)).set(concurrency as f64);
        }

        #[allow(clippy::comparison_chain)]
        if self.tasks.len() == concurrency {
            #[allow(clippy::needless_return)]
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

    pub fn shutdown(mut self) {
        self.set_concurrency(0);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rand_distr::{Distribution, SkewNormal};
    use std::time::Duration;

    #[macro_export]
    macro_rules! mock_scenario {
        ($m:expr, $s:expr) => {
            || async {
                let labels = balter_core::TransactionLabels {
                    success: "",
                    error: "",
                    latency: "",
                };
                let mean: std::time::Duration = $m;
                let std: std::time::Duration = $s;
                let _ = $crate::transaction::transaction_hook::<_, (), ()>(labels, async {
                    let normal =
                        SkewNormal::new(mean.as_secs_f64(), std.as_secs_f64(), 20.).unwrap();
                    let v: f64 = normal.sample(&mut rand::thread_rng()).max(0.);
                    tokio::time::sleep(std::time::Duration::from_secs_f64(v)).await;
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
            "",
            mock_scenario!(Duration::from_millis(1), Duration::from_micros(10)),
            NonZeroU32::new(1_000).unwrap(),
        )
        .await;

        sampler.set_concurrency(20);

        let sample = sampler.sample().await;
        assert!(sample.tps >= 990. && sample.tps <= 1_010.);
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_noisy() {
        let mut sampler = BaseSampler::new(
            "",
            mock_scenario!(Duration::from_millis(10), Duration::from_millis(5)),
            NonZeroU32::new(1_000).unwrap(),
        )
        .await;

        sampler.set_concurrency(210);

        let sample = sampler.sample().await;
        assert!(sample.tps >= 900. && sample.tps <= 1100.);
    }

    /*
    #[tracing_test::traced_test]
    #[tokio::test]
    async fn test_slow() {
        let mut sampler = BaseSampler::new(
            "",
            mock_scenario!(Duration::from_millis(400), Duration::from_millis(100)),
            NonZeroU32::new(50).unwrap(),
        )
        .await;

        sampler.set_concurrency(100);

        let _ = sampler.sample().await;
        let sample = sampler.sample().await;
        dbg!(&sample);
        assert!(sample.tps >= 46. && sample.tps <= 51.);
    }
    */
}
