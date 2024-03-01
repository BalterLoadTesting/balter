#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

pub mod scenario;
#[doc(hidden)]
pub mod transaction;

#[macro_use]
#[doc(hidden)]
pub mod macros;

pub(crate) mod controllers;
pub(crate) mod tps_sampler;

#[cfg(not(feature = "rt"))]
pub use balter_macros::{scenario, transaction};
pub use scenario::Scenario;

cfg_rt! {
    pub use balter_runtime::runtime::{self, BalterRuntime};
    pub use balter_macros::{scenario_linkme as scenario, transaction};
}

pub mod prelude {
    pub use crate::scenario::ConfigurableScenario;
    cfg_rt! {
        pub use balter_runtime::runtime::{distributed_slice, BalterRuntime};
        pub use balter_runtime::traits::DistributedScenario;
        pub use balter_macros::{scenario_linkme as scenario, transaction};
    }

    #[cfg(not(feature = "rt"))]
    pub use balter_macros::{scenario, transaction};

    pub use balter_core::stats::RunStatistics;
}
