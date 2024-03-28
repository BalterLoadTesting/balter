use crate::stats::SampleSet;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

pub trait Controller: Serialize + for<'a> Deserialize<'a> {
    type Lim: Limiter;

    fn new(&self) -> Self::Lim;
}

pub trait Limiter: Sync + Send {
    fn analyze(&mut self, samples: SampleSet) -> NonZeroU32;
}
