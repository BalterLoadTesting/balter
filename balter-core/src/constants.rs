use std::num::NonZeroU32;
use std::time::Duration;

pub const BASE_TPS: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(256) };
pub const BASE_CONCURRENCY: usize = 4;
pub const BASE_INTERVAL: Duration = Duration::from_millis(200);

pub const MIN_SAMPLE_COUNT: u64 = 256;
pub const ADJUSTABLE_SAMPLE_COUNT: u64 = 5_000;
