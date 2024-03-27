use std::num::NonZeroU32;

pub const BASELINE_TPS: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(256) };

/// The default error rate used for `.saturate()`
pub const DEFAULT_SATURATE_ERROR_RATE: f64 = 0.03;

/// The default error rate used for `.overload()`
pub const DEFAULT_OVERLOAD_ERROR_RATE: f64 = 0.80;
