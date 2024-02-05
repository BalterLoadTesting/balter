use humantime::format_duration;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::task::JoinSet;
use tracing::{debug, trace};

const SAMPLE_WINDOW: usize = 20;

pub(crate) struct BatchSizeController {
    compensation_time: Duration,
    batch_size: u64,
    samples: VecDeque<Duration>,
}

impl BatchSizeController {
    pub(crate) async fn new() -> Self {
        let compensation_time = find_approximate_task_spawn_timing_compensation().await;
        debug!(
            "Setting tokio::spawn compensation time to {}.",
            format_duration(compensation_time)
        );
        Self {
            compensation_time,
            batch_size: 1,
            samples: VecDeque::new(),
        }
    }

    pub(crate) fn batch_size(&self) -> u64 {
        self.batch_size
    }

    pub(crate) fn push(&mut self, sample: Duration) {
        self.samples.push_back(sample);
        if self.samples.len() > SAMPLE_WINDOW {
            let _ = self.samples.pop_front();

            if self.analyze() {
                self.samples.clear();
            }
        }
    }

    // Returns true iff batch size has changed
    fn analyze(&mut self) -> bool {
        let mean_dur: Duration = self.samples.iter().sum::<Duration>() / self.samples.len() as u32;

        if mean_dur < self.compensation_time {
            let ratio = self.compensation_time.as_nanos() / mean_dur.as_nanos();
            self.batch_size = self.batch_size * ratio as u64 + 1;
            trace!(
                "Increasing batch size to {} ({} - {})",
                self.batch_size,
                format_duration(mean_dur),
                format_duration(self.compensation_time)
            );
            true
        } else if mean_dur > 2 * self.compensation_time {
            if self.batch_size != 1 {
                self.batch_size -= 1;
                trace!(
                    "Reducing batch size to {} ({} - {})",
                    self.batch_size,
                    format_duration(mean_dur),
                    format_duration(self.compensation_time)
                );
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

async fn find_approximate_task_spawn_timing_compensation() -> Duration {
    let mut set = JoinSet::new();

    let mut time = Duration::new(0, 0);
    for _ in 0..100 {
        let start = Instant::now();
        set.spawn(async move { std::hint::black_box(0) });
        if let Some(res) = set.join_next().await {
            // TODO: Proper error
            let _ = res.expect("JoinSet failure during timing measurements.");
        }
        time += start.elapsed();
    }

    time * 10
}
