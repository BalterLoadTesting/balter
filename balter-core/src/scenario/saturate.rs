use super::{BoxedFut, ScenarioConfig};
#[cfg(feature = "rt")]
use crate::runtime::BALTER_OUT;
use crate::transaction::{TransactionData, TRANSACTION_HOOK};
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

const PROPORTIONAL_CONTROL: f64 = 0.8;

#[instrument(name="saturate", skip_all, fields(name=config.name, error_rate=error_rate))]
pub(crate) async fn run_saturate(
    scenario: fn() -> BoxedFut,
    config: ScenarioConfig,
    error_rate: f64,
) {
    info!(
        "Running {} with {}% error rate for {}",
        config.name,
        error_rate * 100.,
        format_duration(config.duration)
    );

    let start = Instant::now();
    let mut timer = Instant::now();

    let mut task_learner = SaturateTaskLearner::error_rate(error_rate, &config, &start);
    let mut transaction_data = TransactionData {
        limiter: task_learner.limiter.clone(),
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
            // TODO: We should only change this when needed. Frankly, maybe it should just be a
            // part of the task learner.
            transaction_data.limiter = task_learner.limiter.clone();
            timer = Instant::now();
        }

        // Repopulate the jobs
        while jobs.len() < task_learner.task_count() && start.elapsed() < config.duration {
            jobs.spawn(spawn_scenario(scenario, transaction_data.clone()));
        }
    }

    debug!("Scenario complete.");
}

fn handle_statistics(
    transaction_data: &TransactionData,
    task_learner: &mut SaturateTaskLearner,
    elapsed: Duration,
) {
    // We fetch & reset counters to get a rolling average.
    let success_count = transaction_data.success.fetch_min(0, Ordering::Relaxed);
    let error_count = transaction_data.error.fetch_min(0, Ordering::Relaxed);
    let total_count = success_count + error_count;
    let actual_tps = total_count as f64 / elapsed.as_millis() as f64 * 1000.;

    task_learner.push_statistic(actual_tps, success_count, error_count);

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

// TODO: We should be able to combine this with the GoalTPS TaskLearner
// TODO: The implementation of this ought to rely on multiple approach states (eg. linear
// interpolation or proportional approach), which is best implemented with an explicit state
// machine. Currently I'm just hacking it onto the Optional Limiter state, but this is not ideal.
// TODO: This encapsulates way too much logic; need to figure out the abstractions to pull out
#[allow(dead_code)]
struct SaturateTaskLearner<'a> {
    error_rate: f64,
    task_count: usize,
    measurements: u64,
    samples: Vec<f64>,
    previous: Vec<f64>,

    limiter: Option<Arc<DefaultDirectRateLimiter>>,
    limiting_tps: f64,

    complete: bool,

    // TODO: This needs to be cleaned up. Required to have here to handle the side-effect of
    // getting help with the work.
    config: &'a ScenarioConfig,
    start: &'a Instant,
}

impl<'a> SaturateTaskLearner<'a> {
    fn error_rate(error_rate: f64, config: &'a ScenarioConfig, start: &'a Instant) -> Self {
        Self {
            error_rate,
            task_count: 1,
            measurements: 0,
            samples: vec![],
            previous: vec![],
            limiting_tps: 0.0,
            limiter: None,
            complete: false,
            config,
            start,
        }
    }

    fn push_statistic(&mut self, measured_tps: f64, success_count: u64, error_count: u64) {
        // TODO: This assumes we find the optimal task-count _once_ and never again; but this is
        // not a valid assumption given that the server workload can vary. We need to continually
        // measure.
        if self.complete {
            return;
        }

        self.samples.push(measured_tps);
        self.measurements += success_count + error_count;
        trace!(
            "Push statistic: sample count={}, measurements={}, measured_tps={measured_tps}, success_count={success_count}, error_count={error_count}",
            self.samples.len(),
            self.measurements
        );

        // TODO: I pulled these values from thin air
        // NOTE: Slightly different from goal_tps (the governor algorithm is a bit slow to
        // converge, so waiting for more sample points).
        if self.measurements > 10 && self.samples.len() > 3 {
            let mean_tps = mean(&self.samples);
            let actual_error_rate = error_count as f64 / (success_count + error_count) as f64;

            debug!("Measured {actual_error_rate}. Goal is {}", self.error_rate);

            // Special case error_rate 0.0  and no limiter, since it implies that our server might
            // not be able to hit the TPS required to generate the error rate
            if actual_error_rate == 0.0 && self.limiter.is_none() {
                // If a previous task count hits more TPS then we're at our limits for this server
                // and need help.
                if let Some(best_task_count) = self.exists_better_previous(mean_tps) {
                    info!("Goal error rate exceeds capability of this server. Setting limit of self to max achieved: {mean_tps} TPS");
                    self.task_count = best_task_count;

                    // TODO: This side-effect should not be here.
                    #[cfg(feature = "rt")]
                    {
                        let mut new_config = self.config.clone();
                        // TODO: This does not take into account transmission time. Logic will have
                        // to be far fancier to properly time-sync various peers on a single
                        // scenario.
                        new_config.duration = self.config.duration - self.start.elapsed();

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

                    // We set a limiting TPS here to avoid sending too many requests in case this
                    // server gets more capacity.
                    self.limiting_tps = mean_tps;
                    self.limiter = Some(Arc::new(RateLimiter::direct(Quota::per_second(
                        NonZeroU32::new(self.limiting_tps.floor() as u32).unwrap(),
                    ))));
                    self.complete = true;
                } else {
                    self.previous.push(mean_tps);
                    self.task_count += 1;
                    debug!("Increasing task count to {}", self.task_count);
                }
            } else {
                let error_rate_delta = self.error_rate - actual_error_rate;

                // TODO: This logic is hard to follow
                if error_rate_delta.abs() < 0.03 {
                    info!(
                        "Hit near goal with error_rate={:.2}% at {measured_tps} TPS",
                        actual_error_rate * 100.
                    );
                    self.complete = true;
                } else if error_rate_delta.is_sign_positive() {
                    if self.limiter.is_none() {
                        self.task_count += 1;
                        debug!("Increasing task count to {}", self.task_count);
                    } else {
                        let proportional_adjustment =
                            self.limiting_tps * error_rate_delta * PROPORTIONAL_CONTROL;
                        self.limiting_tps += proportional_adjustment;
                        self.limiter = Some(Arc::new(RateLimiter::direct(Quota::per_second(
                            NonZeroU32::new(self.limiting_tps.floor() as u32).unwrap(),
                        ))));
                        debug!(
                            "Actual error rate under goal error rate, adjusting limit to {} TPS",
                            self.limiting_tps
                        );
                    }
                } else if self.limiter.is_none() {
                    // TODO: What about when all requests are errors? What do we do there?
                    let extrapolated_goal_tps = (1. + error_rate_delta) * measured_tps;
                    self.limiting_tps = extrapolated_goal_tps;
                    self.limiter = Some(Arc::new(RateLimiter::direct(Quota::per_second(
                        NonZeroU32::new(self.limiting_tps.floor() as u32).unwrap(),
                    ))));
                    debug!(
                        "Actual error rate exceeds goal error rate, limiting to {} TPS",
                        self.limiting_tps
                    );
                } else {
                    let proportional_adjustment =
                        self.limiting_tps * error_rate_delta * PROPORTIONAL_CONTROL;
                    self.limiting_tps += proportional_adjustment;
                    self.limiter = Some(Arc::new(RateLimiter::direct(Quota::per_second(
                        NonZeroU32::new(self.limiting_tps.floor() as u32).unwrap(),
                    ))));
                    debug!(
                        "Actual error rate exceeds goal error rate, adjusting limit to {} TPS",
                        self.limiting_tps
                    );
                }
            }

            // We want to reset our measurements so we have a rolling average
            self.samples = vec![];
            self.measurements = 0;
        }
    }

    fn task_count(&self) -> usize {
        self.task_count
    }

    fn exists_better_previous(&self, measured_tps: f64) -> Option<usize> {
        let better_counts = self
            .previous
            .iter()
            .enumerate()
            // NOTE: Subtle; need to convert from 0-index to task_count.
            .map(|(idx, x)| (idx + 1, x))
            .filter(|(_, x)| **x > measured_tps)
            .collect::<Vec<_>>();

        if !better_counts.is_empty() {
            let best_count = better_counts
                .iter()
                .max_by_key(|(_, x)| **x as u64)
                .unwrap();
            Some(best_count.0)
        } else {
            None
        }
    }
}

fn mean(samples: &[f64]) -> f64 {
    let sum: f64 = samples.iter().sum();
    sum / samples.len() as f64
}
