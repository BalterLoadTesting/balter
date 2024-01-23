use super::{BoxedFut, ScenarioConfig};
use crate::transaction::{TransactionData, TRANSACTION_HOOK};
#[cfg(feature = "rt")]
use crate::{runtime::BALTER_OUT, scenario::ScenarioKind};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use humantime::format_duration;
use std::{
    future::Future,
    num::NonZeroU32,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
#[cfg(feature = "rt")]
use tokio::runtime::Handle;
use tokio::task::JoinSet;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, Instrument};

#[instrument(name="goal_tps", skip_all, fields(name=config.name, goal_tps=goal_tps))]
pub(crate) async fn run_goal_tps(
    scenario: fn() -> BoxedFut,
    config: ScenarioConfig,
    goal_tps: u32,
) {
    info!(
        "Running {} at {goal_tps}tps for {}",
        config.name,
        format_duration(config.duration)
    );

    let start = Instant::now();
    let mut timer = Instant::now();

    let mut task_learner = GoalTpsTaskLearner::new(goal_tps, &config, &start);
    let mut transaction_data = TransactionData {
        limiter: Some(task_learner.limiter.clone()),
        success: Arc::new(0.into()),
        error: Arc::new(0.into()),
    };

    let mut jobs = JoinSet::new();
    jobs.spawn(spawn_scenario(scenario, transaction_data.clone()));
    #[allow(clippy::redundant_pattern_matching)]
    while let Some(_) = jobs.join_next().await {
        let elapsed = timer.elapsed();
        if elapsed > Duration::from_millis(1000) {
            handle_statistics(&transaction_data, &mut task_learner, elapsed);

            // TODO: We should only change this when needed.
            transaction_data.limiter = Some(task_learner.limiter.clone());
            timer = Instant::now();
        }

        while jobs.len() < task_learner.task_count() && start.elapsed() < config.duration {
            jobs.spawn(spawn_scenario(scenario, transaction_data.clone()));
        }
    }

    debug!("Scenario complete.");
}

fn handle_statistics(
    transaction_data: &TransactionData,
    task_learner: &mut GoalTpsTaskLearner,
    elapsed: Duration,
) {
    // Fetch & reset counters and timer.
    let success_count = transaction_data.success.fetch_min(0, Ordering::Relaxed);
    let error_count = transaction_data.error.fetch_min(0, Ordering::Relaxed);
    let total_count = success_count + error_count;
    let actual_tps = (success_count + error_count) as f64 / elapsed.as_millis() as f64 * 1000.;

    // We need to recalculate whether or not to increase task count
    task_learner.push_statistic(actual_tps, total_count);

    // TODO: We should log metrics at this point (or possibly in the transaction hook)
}

fn spawn_scenario(
    scenario: fn() -> BoxedFut,
    transaction_data: TransactionData,
) -> impl Future<Output = ()> + Send {
    TRANSACTION_HOOK
        .scope(transaction_data, async move { scenario().await })
        .in_current_span()
}

/// Calculates the optimal number of concurrent tasks to run
///
/// The gist of how it works is to keep a rolling average of the TPS that we are able to achieve,
/// and if we're lower than the goal_tps we continually increase the number of tasks. However, if
/// we increase the number of tasks and are not reaching a higher TPS than a lower number of
/// concurrent tasks then we stop and assume our service can not handle the goal_tps.
///
/// There are a number of limitations to the current design, with the cheif being that once we
/// determine an "optimal" task count we just stop measuring anything. This is not ideal, as the
/// scenarios being run could change over time (meaning we are no longer hitting the TPS, or that
/// we have capacity). Additionally I'm sure this could be made more efficient.
///
/// TODO: Merge with SaturateTaskLearner.
#[allow(dead_code)]
struct GoalTpsTaskLearner<'a> {
    samples: Vec<f64>,
    measurements: u64,
    task_count: usize,
    previous: Vec<f64>,
    complete: bool,

    limiter: Arc<DefaultDirectRateLimiter>,
    goal_tps: f64,

    // TODO: This needs to be cleaned up. Required to have here to handle the side-effect of
    // getting help with the work.
    config: &'a ScenarioConfig,
    start: &'a Instant,
}

impl<'a> GoalTpsTaskLearner<'a> {
    fn new(goal_tps: u32, config: &'a ScenarioConfig, start: &'a Instant) -> Self {
        Self {
            samples: vec![],
            measurements: 0,
            task_count: 1,
            previous: vec![],
            complete: false,

            limiter: Arc::new(RateLimiter::direct(Quota::per_second(
                NonZeroU32::new(goal_tps).unwrap(),
            ))),
            goal_tps: goal_tps as f64,

            config,
            start,
        }
    }

    fn push_statistic(&mut self, measured_tps: f64, measurements: u64) {
        // TODO: This assumes we find the optimal task-count _once_ and never again; but this is
        // not a valid assumption given that the server workload can vary. We need to continually
        // measure.
        if self.complete {
            return;
        }

        self.samples.push(measured_tps);
        self.measurements += measurements;
        trace!(
            "Push statistic: sample count={}, measurements={}",
            self.samples.len(),
            self.measurements
        );

        // TODO: I pulled these values from thin air. The goal is to wait until we have enough data
        // to actually make statistically reasonable decisions.
        if self.measurements > 10 || self.samples.len() > 5 {
            let mean_tps = mean(&self.samples);

            // Check if we're under by more than 5%. We ignore going over, as we'll let the
            // limiter handle that (it will eventually). Additionally we want to cut it a
            // bit of slack in getting it super accurate.
            let error = ((self.goal_tps - mean_tps) / self.goal_tps).max(0.0);
            if error > 0.05 {
                // TODO: We can use better math here to determine if we are no longer increasing in
                // mean_tps given more task count (ie. our server is overloaded and increasing
                // tasks will just make it worse). Really its just looking at where the derivative
                // is near 0. Also this iterator is pgross.
                let better_counts = self
                    .previous
                    .iter()
                    .enumerate()
                    // NOTE: Subtle; need to convert from 0-index to task_count.
                    .map(|(idx, x)| (idx + 1, x))
                    .filter(|(_, x)| **x > mean_tps)
                    .collect::<Vec<_>>();
                if !better_counts.is_empty() {
                    // TODO: Ord doesn't work for floats; just doing a dumb cast to u64 which is
                    // not ideal.
                    let best_count = better_counts
                        .iter()
                        .max_by_key(|(_, x)| **x as u64)
                        .unwrap();
                    info!("Goal TPS exceeds capability of this server. Found best task count: {best_count:?}.");

                    // TODO: Move side-effect out of here.
                    #[cfg(feature = "rt")]
                    {
                        let mut new_config = self.config.clone();
                        // TODO: This does not take into account transmission time. Logic will have
                        // to be far fancier to properly time-sync various peers on a single
                        // scenario.
                        new_config.duration = self.config.duration - self.start.elapsed();
                        match &mut new_config.kind {
                            ScenarioKind::Tps(ref mut goal_tps) => {
                                // TODO: Need better checks that this doesn't get set to 0
                                *goal_tps = (self.goal_tps - mean_tps).floor() as u32;
                            }
                            _ => unreachable!(),
                        }

                        let handle = Handle::current();
                        handle.spawn(async move {
                            let (ref tx, _) = *BALTER_OUT;
                            // TODO: Handle the error case.
                            let _ = tx.send(new_config).await;
                        });
                    }

                    #[cfg(not(feature = "rt"))]
                    {
                        error!("No distributed runtime available to scale out.");
                    }

                    self.goal_tps = mean_tps;
                    self.limiter = Arc::new(RateLimiter::direct(Quota::per_second(
                        NonZeroU32::new(self.goal_tps.floor() as u32).unwrap(),
                    )));

                    self.task_count = best_count.0;
                    self.complete = true;
                } else {
                    self.previous.push(mean_tps);
                    self.task_count += 1;
                    debug!(
                        "Measured {mean_tps}, increasing task count to {}",
                        self.task_count
                    );
                }
            } else {
                debug!("Found task count: {}", self.task_count);
                self.complete = true;
            }

            // We *must* reset our measurements so we have a sortof rolling average.
            self.samples = vec![];
            self.measurements = 0;
        }
    }

    fn task_count(&self) -> usize {
        self.task_count
    }
}

fn mean(samples: &[f64]) -> f64 {
    let sum: f64 = samples.iter().sum();
    sum / samples.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scaling_up() {
        let config = ScenarioConfig {
            name: "some_task".to_string(),
            duration: Duration::from_secs(30),
            kind: ScenarioKind::Tps(100),
        };
        let start = Instant::now();
        let mut task_learner = GoalTpsTaskLearner::new(100, &config, &start);

        assert_eq!(task_learner.task_count(), 1);

        task_learner.push_statistic(50., 700);

        assert_eq!(task_learner.task_count(), 2);
    }

    #[tokio::test]
    async fn test_dont_overload() {
        let config = ScenarioConfig {
            name: "some_task".to_string(),
            duration: Duration::from_secs(30),
            kind: ScenarioKind::Tps(100),
        };
        let start = Instant::now();
        let mut task_learner = GoalTpsTaskLearner::new(100, &config, &start);

        assert_eq!(task_learner.task_count(), 1);
        task_learner.push_statistic(50., 700);

        assert_eq!(task_learner.task_count(), 2);
        task_learner.push_statistic(75., 700);

        assert_eq!(task_learner.task_count(), 3);
        task_learner.push_statistic(80., 700);

        assert_eq!(task_learner.task_count(), 4);
        task_learner.push_statistic(85., 700);

        assert_eq!(task_learner.task_count(), 5);
        task_learner.push_statistic(73., 700);

        assert_eq!(task_learner.task_count(), 4);

        // NOTE: If tests are hanging, its probably because this is broken
        let (_, ref rx) = *BALTER_OUT;
        assert!(rx.recv().await.is_ok());
    }
}
