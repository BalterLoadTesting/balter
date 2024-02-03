#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

#[macro_use]
#[doc(hidden)]
pub mod macros;

cfg_rt! {
    pub mod runtime;
}
pub(crate) mod sampling;
pub mod scenario;
#[doc(hidden)]
pub mod transaction;

cfg_rt! {
    mod gossip;
}
cfg_rt! {
    mod server;
}

cfg_rt! {
    pub use crate::runtime::BalterRuntime;
}

cfg_rt! {
    pub use balter_macros::{scenario_linkme as scenario, transaction};
}
#[cfg(not(feature = "rt"))]
pub use balter_macros::{scenario, transaction};

pub use scenario::Scenario;

pub mod prelude {
    pub use crate::scenario::ConfigurableScenario;
    cfg_rt! {
        pub use crate::runtime::{distributed_slice, BalterRuntime};
    }

    cfg_rt! {
        pub use balter_macros::{scenario_linkme as scenario, transaction};
    }
    #[cfg(not(feature = "rt"))]
    pub use balter_macros::{scenario, transaction};
}
