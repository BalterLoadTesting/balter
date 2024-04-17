# Balter

[![Linux build status](https://github.com/byronwasti/balter/workflows/CI/badge.svg)](https://github.com/byronwasti/balter/actions)
[![Crates.io](https://img.shields.io/crates/v/balter.svg)](https://crates.io/crates/balter)

Balter, short for *Balter, A Load TestER*, is a load/stress testing framework designed to be flexible, efficient, and simple to use. Balter aims to minimize the conceptual overhead of load testing, and builds off of Tokio and the async ecosystem.

- See the [Website](https://www.balterloadtesting.com/) for an introduction to Balter.
- See the [Guide](https://www.balterloadtesting.com/guide) for a guide on how to get started.
- See the [Developer Notes](#developer-notes) section for tips on modifying Balter.

# Example Usage

```rust,no_run
use balter::prelude::*;
use std::time::Duration;

#[tokio::main]
async fn main() {
    my_scenario()
        .tps(500)
        .error_rate(0.05)
        .latency(Duration::from_millis(20), 0.99)
        .duration(Duration::from_secs(30))
        .await;
}

#[scenario]
async fn my_scenario() {
    my_transaction().await;
}

#[transaction]
async fn my_transaction() -> Result<u32, String> {
    // Some request logic...

    Ok(0)
}
```

## Developer Notes

The Balter repository is set up to be easy to get started with development. It uses Nix to facilitate the environment setup via `shell.nix` (if you haven't yet drank the Nixaide, open up that file and it will give you an idea of the programs you'll want).

To run the integration tests, use `just integration` (or if you don't have `just` installed, `cargo test --release --features integration`). In order to easily debug these tests (which oftentimes rely on controller logic operating correctly), it can be useful to have graphs. You can find Grafana dashboards for each test in `dashboards/`, and if you have Prometheus running (using the `prometheus.yml` at the root) and Grafana running (importing the dashboards) you should be set.
