+++
title = "Index"
template = "index.html"
+++

# Balter
<b>B</b>alter, <b>A</b> <b>L</b>oad <b>T</b>est<b>ER</b>, is a distributed load testing framework designed to make testing high-traffic scenarios easy. Load test scenarios are written using standard Rust code, with a few special attributes thrown in. Balter makes no assumptions about the service under test, and can be used for a variety of use-cases, from HTTP services to local key-value stores written in any language.

- Flexible and composable testing via Scenario and Transaction abstractions.
- Constrain load tests with max TPS, latency or error rate (including all three at once).
- Distributed runtime in just a few lines of code.*
- Native integration with Prometheus for easy metrics in Grafana.
- Written with efficiency in mind. Don't break the bank with load testing.

\* Experimental feature, but high priority for stabilizing!


# How It Works

At its core, Balter provides tooling to take an arbitrary Rust function and scale it. The two abstractions Balter provides are the *Scenario*, an async Rust function with the `#[scenario]` attribute, and the *Transaction*, an async Rust function with the `#[transaction]` attribute:

```rust
#[scenario]
async fn test_scaling_functionality() {
    loop {
        foo_transaction().await;
    }
}

#[transaction]
async fn foo_transaction() -> Result<()> {
    // Service call would go here
    Ok(())
}
```

A *Scenario* supercharges a Rust function with additional methods related to load testing. All of the methods run multiple instances of the function in parallel, while keeping track of various statistical data around the transactions. Balter provides the following methods for a Scenario:

- `.tps(u32)` Run a Scenario such that the transactions per second is equal to the value set.
- `.error_rate(f64)` Run a Scenario such that the error rate is equal to the value set.
- `.latency(Duration, f64)` Run a Scenario given a latency and a percentile.
- `.duration(Duration)` Limit the Scenario to run for a given Duration (otherwise it runs indefinitely)

These methods can be used together. For example, let's say you want to scale a function to achieve a p90 latency of 200ms, but not go over 10,000 TPS or an error rate of 3%, and run it for 3600s:
```rust
test_scaling_functionality()
    .latency(Duration::from_millis(200), 0.90)
    .tps(10_000)
    .error_rate(0.03)
    .duration(Duration::from_secs(3600))
    .await;
```

# Composability

What sets Balter apart from other load testing frameworks like JMeter or Locust is composability. Scenarios are normal async Rust functions, and this opens up a world of flexibility.

For example, say you want to run a series of load tests, you can run Scenarios in series:
```rust
test_normal_user_load()
    .tps(10_000)
    .error_rate(0.03)
    .duration(Duration::from_secs(3600))
    .await;

sleep(Duration::from_secs(3600)).await;

test_edge_cases()
    .latency(Duration::from_millis(100), 0.99)
    .duration(Duration::from_secs(3600))
    .await;
```

Where things get interesting is the ability to run Scenarios in parallel, using the standard Tokio `join!` macro:

```rust
tokio::join! {
    async {
        // First, set up a background load which either hits
        // 10K TPS, has a p95 latency of 200ms or has an
        // error rate of 5%
        set_background_load()
            .tps(10_000)
            .latency(Duration::from_millis(200), 0.95)
            .error_rate(0.05)
            .await;
    },
    async {
        // After 300s of waiting, test our scaling ability
        // by running a scenario which achieves either
        // 100K TPS or a p90 latency of 1,000ms
        sleep(Duration::from_secs(300)).await;

        test_scaling_functionality()
            .tps(100_000)
            .latency(Duration::from_millis(1_000), 0.90)
            .duration(Duration::from_secs(3600))
            .await;
    },
}
```

Of course, you aren't limited to just running Balter Scenarios. For instance, you can make API calls to disable certain services while a load test is running.

To learn more, and see advanced Balter strategies, see [the guide](@/guide/_index.md).

# Distributed Runtime (Experimental)

Balter provides an distributed runtime if your load testing requirements are higher than what a single machine can handle. This runtime is currently in an experimental state, though stabilization is a high priority for the near future. To get started with the distributed runtime, you will need to opt-in to the `"rt"` feature (and include `linkme` as a dependency):

```toml
# Cargo.toml
balter = { version = "0.0.5", features = ["rt"] }
linkme = "0.3"
```

Then, rather than calling a Scenario from your `main()` function, you instantiate the Runtime, which will automatically hook into the various Scenario's you have.

```rust
#[tokio::main]
async fn main() {
    BalterRuntime::new()
        .with_args()
        .run()
        .await;
}
```

In the background, the Balter runtime starts an HTTP server which you can send requests to. In order to kick off a Scenario, you simply send an HTTP request.

More about this topic is covered in the [guide](@/guide/_index.md#distributed-runtime-experimental).

# Native Metrics


# Support

The easiest way to support Balter is by giving us a star on Github! This helps people find us and learn about the project. <a class="github-button" href="https://github.com/BalterLoadTesting/balter" data-size="large" data-show-count="true" aria-label="Star BalterLoadTesting/balter on GitHub">Star</a>

The best way to financially support Balter's development is to hire us! We provide contracting for building out your team's load testing infrastructure using Balter. You will have full access and ownership over the source code, using all Open Source technology, and have complete confidence in the performance and load characteristics of your system. At the same time, you will be supporting the project and giving us insights into how to improve the framework.

If you are interested in hiring us, please reach out to <a href="mailto:consulting@balterloadtesting.com">consulting@balterloadtesting.com</a>

Additionally, you can sponsor our developers on Github, which is a great way to show support, especially if there are features you're interested in seeing added!
<iframe src="https://github.com/sponsors/byronwasti/card" title="Sponsor byronwasti" height="100" width="600" style="border: 0;"></iframe>

