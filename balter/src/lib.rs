#![cfg_attr(docsrs, feature(doc_cfg))]
//! # Balter
//!
//! *Balter, A Load TestER*, is a load/stress testing framework designed to be flexible, efficient, and simple to use. Balter aims to minimize the conceptual overhead of load testing, and builds off of Tokio and the async ecosystem.
//!
//! - See the [Website](https://www.balterloadtesting.com/) for an introduction to Balter.
//! - See the [Guide](https://www.balterloadtesting.com/guide) for a guide on how to get started.
//! # Example Usage
//!
//! ```rust,no_run
//! use balter::prelude::*;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     my_scenario()
//!         .tps(500)
//!         .error_rate(0.05)
//!         .latency(Duration::from_millis(20), 0.99)
//!         .duration(Duration::from_secs(30))
//!         .await;
//! }
//!
//! #[scenario]
//! async fn my_scenario() {
//!     my_transaction().await;
//! }
//!
//! #[transaction]
//! async fn my_transaction() -> Result<u32, String> {
//!     // Some request logic...
//!
//!     Ok(0)
//! }
//! ```
pub mod scenario;
#[doc(hidden)]
pub mod transaction;

mod hints;

#[macro_use]
#[doc(hidden)]
pub mod macros;

pub(crate) mod controllers;
pub(crate) mod measurements;
pub(crate) mod sampler;

#[cfg(not(feature = "rt"))]
pub use balter_macros::{scenario, transaction};
pub use hints::Hint;
pub use scenario::Scenario;

cfg_rt! {
    pub use balter_runtime::runtime::{self, BalterRuntime};
    pub use balter_macros::{scenario_linkme as scenario, transaction};
}

#[doc(hidden)]
pub mod core {
    pub use balter_core::*;
}

pub use core::RunStatistics;

pub mod prelude {
    pub use crate::scenario::ConfigurableScenario;
    cfg_rt! {
        pub use balter_runtime::runtime::{distributed_slice, BalterRuntime};
        pub use balter_runtime::traits::DistributedScenario;
        pub use balter_macros::{scenario_linkme as scenario, transaction};
    }

    #[cfg(not(feature = "rt"))]
    pub use balter_macros::{scenario, transaction};

    pub use balter_core::RunStatistics;
}
