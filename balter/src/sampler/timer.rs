use std::time::Duration;
use tokio::time::{interval, Instant, Interval};
#[allow(unused)]
use tracing::{debug, error, info, trace, warn};

pub(crate) struct Timer {
    interval: Interval,
    last_tick: Instant,
    interval_dur: Duration,
}

impl Timer {
    pub async fn new(interval_dur: Duration) -> Self {
        let mut interval = interval(interval_dur);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // NOTE: First tick completes instantly
        let last_tick = interval.tick().await;
        Self {
            interval,
            last_tick,
            interval_dur,
        }
    }

    pub async fn tick(&mut self) -> Duration {
        let next = self.interval.tick().await;
        let elapsed = self.last_tick.elapsed();
        self.last_tick = next;
        elapsed
    }

    #[allow(unused)]
    pub async fn set_interval_dur(&mut self, dur: Duration) {
        if dur < Duration::from_secs(10) {
            *self = Self::new(dur).await;
        } else {
            error!("Balter's polling interval is greater than 10s. This is likely a sign of an issue; not increasing the polling interval.")
        }
    }

    #[allow(unused)]
    pub fn interval_dur(&self) -> Duration {
        self.interval_dur
    }

    #[allow(unused)]
    pub async fn double(&mut self) {
        if self.interval_dur < Duration::from_secs(10) {
            self.interval_dur *= 2;
            *self = Self::new(self.interval_dur).await;
        } else {
            error!("Balter's Sampling interval is greater than 10s. This is likely a sign of an issue; not increasing the sampling interval.")
        }
    }
}

impl std::fmt::Display for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", humantime::format_duration(self.interval_dur))
    }
}
