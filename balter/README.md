# Balter

[![Linux build status](https://github.com/byronwasti/balter/workflows/CI/badge.svg)](https://github.com/byronwasti/balter/actions)
[![Crates.io](https://img.shields.io/crates/v/balter.svg)](https://crates.io/crates/balter)

Balter, short for *Build A Load TestER*, is a load/stress testing framework for Rust designed to be flexible, efficient, and simple to use. Balter aims to minimize the conceptual overhead of load testing, and builds off of Tokio and the async ecosystem.

- See the [How To Use](#how-to-use) section for a guide on how to get started.
- See the [Limitations](#limitations) section for current things to be aware of (this is Beta software).
- See the [How It Works](#how-it-works) and [Developer Notes](#developer-notes) section for tips on modifying Balter.

## Features

- Write load tests using normal Rust code
- Run scenarios with a given transaction per second (TPS)
- Run scenarios to find the max TPS at a given error rate
- Run scenarios distributed across multiple machines

## WIP

- [ ] More robust logic
- [ ] mTLS
- [ ] Peer discovery via DNS
- [ ] Autoscaling hooks
- [ ] More customizability (eg. keyed transaction limits)
- [ ] Efficiency improvements

## How To Use

Balter is designed to be simple to get started with while allowing for advanced workflows. At the core of the abstraction are two concepts: (1) the _transaction_ and (2) the _scenario_.

First you need to include Balter in your `Cargo.toml`.

```toml
[dependencies]
balter = "0.3"
```

A _transaction_ is a single request to your service.
A transaction is the way Balter measures the number and timing of requests being made to the service, as well as the error rate and success rate.
These are used to make various scaling decisions.
You denote a transaction with the `#[transaction]` macro.
Currently Balter only supports transactions which are async functions with any number of arguments that return a `Result<T, E>`, though this will be made more flexible in the future (such as supporting `Option<T>` or infallible transactions).

```rust,ignore
use balter::prelude::*;

#[transaction]
async fn my_transaction(foo: Foo) -> Result<Bar, MyError> {
    ...
}

#[transaction]
async fn other_transaction(foo: Foo, foo2: Foo2, ...) -> Result<Bar, MyError> {
    ...
}
```

A _scenario_ is a function which calls any number of _transactions_ (or other scenarios).
Similar to transactions, a scenario is denoted with the `#[scenario]` macro.
By default, scenarios behave identically to normal async Rust functions.

```rust,ignore
use balter::prelude::*;

#[scenario]
async fn my_scenario() {
    let _ = my_transaction().await;
}

#[scenario]
async fn my_other_scenario() {
    let res = my_transaction().await;

    for i in res.count {
        other_transaction().await;
    }
}

#[scenario]
async fn my_nested_scenario() {
    my_scenario().await;
    my_other_scenario().await;
}
```

What makes scenarios different from regular async functions is that they have additional methods allowing you to specify load testing functionality.

- `.tps(u32)` Run a scenario at a specified TPS
- `.error_rate(f64)` Run a scenario, increasing the TPS until a custom error rate

```rust,ignore
// Run scenario at 300 TPS for 30 seconds
my_scenario().tps(300u32).duration(Duration::from_secs(30)).await;

// Run a scenario increasing the TPS until a specified error rate:
// Increase TPS until we see an error rate of 25% for 120 seconds
my_scenario().error_rate(0.25).duration(Duration::from_secs(120)).await;
```

You can run scenarios together for more complicated load test scenarios using standard tokio async code:

```rust,ignore
use balter::prelude::*;

#[scenario]
async fn my_root_scenario() {
    // In series
    my_scenario_a().tps(300).duration(Duration::from_secs(120));
    my_scenario_b().saturate().duration(Duration::from_secs(120));

    // In parallel
    tokio::join! {
        my_scenario_a().tps(300).duration(Duration::from_secs(120)),
        my_scenario_b().saturate().duration(Duration::from_secs(120)),
    }
}
```

Currently Balter only supports scenarios which are async functions which take no arguments and return no values; this unfortunately limits them a bit right now but is a restriction which will be lifted soon. Additionally, scenarios must supply a duration, but this is also a restriction which will be lifted soon (and will result in a scenario which runs indefinitely).

All put together, a simple single-server load test looks like the following:

```rust,no_run
use balter::prelude::*;
use std::time::Duration;

#[tokio::main]
async fn main() {
    my_scenario()
        .tps(500)
        .duration(Duration::from_secs(30))
        .await;

    my_scenario()
        .error_rate(0.03)
        .duration(Duration::from_secs(120))
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

### Distributed Support (Experimental)

Running a load test on a single server is limited, and Balter aims to provide a distributed runtime. Currently Balter supports distributed load tests, but they are fragile and not efficient. This functionality will improve over time, but the current support should be considered experimental.

To use the distributed runtime, you need to set the `rt` feature flag. You will also need to add `linkme` to your dependencies list.

```toml
[dependencies]
balter = { version = "0.3", features = ["rt"] }
linkme = "0.3"
```

The next step is to instantiate the runtime. This is needed in order to set up the server and gossip functionality.

```rust,no_run
use balter::prelude::*;

#[tokio::main]
async fn main() {
    BalterRuntime::new().with_args().run().await;
}
```

Note that we call `.with_args()` on the runtime. This sets up the binary to accept CLI arguments for the port (`-p`) and for peer addresses (`-n`). You can also use the builder pattern with `.port()` and `.peers()`, which are documented in the rustdocs. In order to have distributed load testing support, each instantiation of the service needs to know of the address of at least one peer, otherwise the gossip functionality won't work. Support will be added for DNS support to allow for more dynamic addresses. With the runtime configured, you can spin up the servers.

Assuming the first server is running on `127.0.0.1:7621` (the first server does not need any peer addresses), each subsequent service can be started like so:

```bash
$ ./load_test_binary -n 127.0.0.1:7621
```

Once the services are all pointed at each other, they will begin to gossip and coordinate. To start a load test, you make an HTTP request to the `/run` endpoint of *any* of the services in the mesh with the name being the function name of the scenario you would like to run. Currently you must specify all parameters, and `.saturate()`, `overload()` and `error_rate()` are all under one header (`Saturate`).

```bash
$ # For running a TPS load test
$ curl "127.0.0.1:7621/run" --json '{ "name": "my_scenario", "duration": 30, "kind": { "Tps": 500 }}'

$ # For running a saturate/overload/error_rate load test (`.saturate()` is 0.03, `.overload()` is 0.80)
$ curl "127.0.0.1:7621/run" --json '{ "name": "my_scenario", "duration": 30, "kind": { "Saturate": 0.03 }}'
```

## Limitations

Balter is a Beta framework and there are some rough edges. This section lists the most important to be aware of. All of these are limitations being worked on, as the goal of Balter is to be as flexible as is needed.

- Various type restrictions
    - Scenario must be functions which take no arguments and return no values
    - Transactions must be functions which return a `Result<T, E>`

- The distributed functionality is experimental.
	- Inefficient Gossip protocol being used
	- No transaction security (no TLS, mTLS, or WSS) - use at your own risk (and in a private VPC)
	- Likely to run into weird error cases

## How It Works

Balter works by continuously measuring transaction timings and transaction success rates. It does this via `tokio::task_local!`: the scenarios create hooks via the `task_local!` which the transactions submit data to. Balter uses this data to scale up the TPS on the machine a scenario started on by increasing the number of concurrent tasks.

If the TPS required for a scenario is too high for the given server to handle, it will find the optimal parallel task count to maximize the output TPS of itself. If the distributed runtime is being used, then the server requests help from its peers.

The distributed runtime has two primary tasks being run in the background: (1) the API server and (2) the gossip protocol. The API server is to handle the initial `/run` request, as well as for setting up the websocket support for the gossip protocol.

The gossip protocol is run over websockets, and is fairly crude at this point.
The only state shared about each peer is whether it is free or busy (or down).
A peer is only busy if it has asked other servers for help.
Consensus is done by sending all information between two nodes and each taking the max of the intersection, given that the data is monotonic.

## Developer Notes

The Balter repository is set up to be easy to get started with development. It uses Nix to facilitate the environment setup via `shell.nix` (if you haven't yet drank the Nixaide, open up that file and it will give you an idea of the programs you'll want).

To run the integration tests, use `cargo test --release --features integration`. In order to easily debug these tests (which oftentimes rely on controller logic operating correctly), it can be useful to have graphs. You can find Grafana dashboards for each test in `dashboards/`, and if you have Prometheus running (using the `prometheus.yml` at the root) and Grafana running (importing the dashboards) you should be set.
