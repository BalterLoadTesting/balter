use std::num::NonZeroU32;
use std::time::Duration;

pub const BASE_TPS: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(512) };
pub const BASE_CONCURRENCY: usize = 10;
pub const BASE_INTERVAL: Duration = Duration::from_millis(1000);
pub const BASE_INTERVAL_SLOW: Duration = Duration::from_millis(5000);
