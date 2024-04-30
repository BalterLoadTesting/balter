+++
title = "Index"
template = "index.html"
+++

# Balter
<b>B</b>alter, <b>A</b> <b>L</b>oad <b>T</b>est<b>ER</b>, is a distributed load testing framework designed to make testing high-traffic scenarios easy. Load test scenarios are written using standard Rust code with two special attributes thrown in. Balter makes no assumptions about the service under test, and can be used for a variety of use-cases, from HTTP services to local key-value stores written in any language.

High level features include:

- Flexible and composable testing via Scenario and Transaction abstractions.
- Constrain load tests with TPS, latency or error rate (including all three at once).
- Distributed runtime in just a few lines of code.
- Native metrics integration.
- Written with efficiency in mind. Don't break the bank with load testing.

Balter is a new project and still has some rough edges. The project is being worked on full time, so if you run into any issues please let us know on Github and we will try to resolve them as quickly as possible.

# How It Works

Balter is a framework for writing load tests with regular Rust code. Balter introduces two macros, `#[scenario]` and `#[transaction]` which compose together to make load test scenarios:

```rust
use balter::prelude::*;

#[scenario]
async fn test_scaling_functionality() {
    let client = reqwest::Client::new();
    loop {
        foo_transaction(&client).await;

        for _ in 0..10 {
            bar_transaction(&client).await;
        }
    }
}

#[transaction]
async fn foo_transaction(client: &Client) -> Result<()> {
    client.post("https://example.com/api/foo")
        .json(...)
        .send().await?;
    Ok(())
}

#[transaction]
async fn bar_transaction(client: &Client) -> Result<()> {
    client.post("https://example.com/api/bar")
        .json(...)
        .send().await?;
    Ok(())
}
```

A Scenario supercharges a Rust function with additional methods related to load testing. For instance, the `.tps()` method will run the function in parallel and constrain the rate of transactions such that the transactions per second (TPS) is equal to the value you set:
```rust
test_scaling_functionality()
    .tps(10_000)
    .await;
```

Balter currently provides the following methods for a Scenario:

- `.tps(u32)` Run a Scenario such that the transactions per second is equal to the value set.
- `.error_rate(f64)` Constrain transaction rate to an average error rate.
- `.latency(Duration, f64)` Constrain transaction rate to a specific latency at a given percentile.
- `.duration(Duration)` Limit the Scenario to run for a given Duration (by default it runs indefinitely)

These methods can be used together. For example, let's say you want to scale a function to achieve a p90 latency of 200ms, but not go over 10,000 TPS or an error rate of 3%, and run it for 3600s:
```rust
test_scaling_functionality()
    .latency(Duration::from_millis(200), 0.90)
    .tps(10_000)
    .error_rate(0.03)
    .duration(Duration::from_secs(3600))
    .await;
```

See [the guide](@/guide/_index.md) for more information on the core primitives Balter provides and current restrictions they have.

# Composability

What sets Balter apart from other load testing frameworks like JMeter or Locust is composability. Scenarios are normal async Rust functions, and this opens up a world of flexibility.

For example, you can call Scenarios one after another if you want to run a set of load tests:
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

Where things get interesting is the ability to run Scenarios in parallel, using the standard Tokio `join!` macro. For instance, being able to set up a baseline amount of load against your service, and then slamming it with high traffic for a few minutes, is made simple with Balter:

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

Of course, you aren't limited to just running Balter Scenarios. For example, you can make API calls to disable certain services while a load test is running. The possibilities are endless! Balter aims to provide the minimal abstraction overhead to answer all high-load questions about your service.

# Native Metrics

Metrics are an important part of understanding load performance, and Balter natively supports metrics via the [`metrics` crate](https://github.com/metrics-rs/metrics). This means you can plug in any adapter you need to output metrics in a way that integrates with your system. For instance, Prometheus integration is as easy as adding the following:

```rust
PrometheusBuilder::new()
    .with_http_listener("0.0.0.0:8002".parse::<SocketAddr>()?)
    .install()?;
```

The metrics output by Balter include statistical information on TPS, latency, error-rates as well as information on the inner workings of Balter (such as the concurrency and controller states).

{{ resize_image(path="/static/balter-metrics-demo-1.png", width=5000, height=5000, op="fit") }}

See [the guide](@/guide/_index.md#metrics) for more information.

# Distributed Runtime

Balter provides a distributed runtime if your load testing requirements are higher than what a single machine can handle. This runtime is currently in an experimental state, though stabilization is a high priority for the near future. Complete documentation can be found in [the guide](@/guide/_index.md#distributed-runtime-experimental).

Currently, the runtime just needs a port and at least a single peer (in order to start gossiping with). Then, rather than calling a Scenario from your `main()` function, you instantiate the Runtime, which will automatically hook into the various Scenario's you have.

```rust
#[tokio::main]
async fn main() -> Result<()> {
    BalterRuntime::new()
        .port(7621)
        .peers(&["192.168.0.1".parse()?])
        .run()
        .await;
}
```

In the background, the Balter runtime will start gossiping with peers and sharing work. In order to kick off a Scenario, you simply send an HTTP request to any Balter server, and everything else will be handled automatically. [The guide](@/guide/_index.md#distributed-runtime-experimental) covers more information on the distributed runtime, and the caveats that currently exist.

# Support

Balter is a brand new project, and any support is greatly appreciated!

The easiest way to support Balter is by giving us a star on Github. This helps people find us and learn about the project. <a class="github-button" href="https://github.com/BalterLoadTesting/balter" data-size="large" data-show-count="true" aria-label="Star BalterLoadTesting/balter on GitHub">Star</a>

The best way to financially support Balter's development is to hire us! We provide contracting for building out your team's load testing infrastructure using Balter. You will have full access and ownership over the source code, using all Open Source technology, and have complete confidence in the performance and load characteristics of your system. At the same time, you will be supporting the project and giving us insights into how to improve the framework.

If you are interested in hiring us, please reach out to <a href="mailto:consulting@balterloadtesting.com">consulting@balterloadtesting.com</a>

Additionally, you can sponsor our developers on Github, which is a great way to show support, especially if there are features you're interested in seeing added!
<iframe id="sponsor-big" src="https://github.com/sponsors/byronwasti/card" title="Sponsor byronwasti" height="100" width="600" style="border: 0;"></iframe>
<iframe id="sponsor-small" src="https://github.com/sponsors/byronwasti/button" title="Sponsor byronwasti" height="32" width="114" style="border: 0; border-radius: 6px;"></iframe>

