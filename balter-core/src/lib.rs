#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

#[macro_use]
#[doc(hidden)]
pub mod macros;

cfg_rt! {
    pub mod runtime;
}
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
pub use balter_macros::{scenario, transaction};
pub use scenario::Scenario;

pub mod prelude {
    cfg_rt! {
        pub use crate::runtime::BalterRuntime;
    }
    pub use balter_macros::{scenario, transaction};
}
