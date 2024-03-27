use balter_core::ScenarioConfig;
use std::{future::Future, pin::Pin};

#[doc(hidden)]
pub trait DistributedScenario: Future + Send {
    fn set_config(
        &self,
        config: ScenarioConfig,
    ) -> Pin<Box<dyn DistributedScenario<Output = Self::Output>>>;
}
