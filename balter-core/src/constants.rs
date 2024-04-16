use std::num::NonZeroU32;

pub const BASELINE_TPS: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(256) };
