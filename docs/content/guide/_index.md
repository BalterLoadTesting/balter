+++
title = "Guide"
template = "guide.html"
description = "guide"
+++

# Building Blocks

## Scenarios

The top abstraction in Balter is the *Scenario*. They are asynchronous functions with a `#[scenario]` attribute macro, and contain the logic for the test you want to run against your service.

For example, the following is a simple Scenario, which calls the same transaction repeatedly,

```rust
#[scenario]
async fn scenario_foo() {
    loop {
        let _ = call_fetch_endpoint().await;
    }
}
```



### Current Restrictions
- `#[scenario]` can only be used on functions which take and return no arguments ([issue #1](https://github.com/BalterLoadTesting/balter/issues/1))

## Transactions

## Basic Example

```rust
use balter::prelude::*;

#[tokio::main]
async fn main() {
    // Run a scenario in parallel for 3600s,
    // constraining the TPS such that:
    // - Max 5,000 transactions per second
    // - Max p95 latency is 20ms
    // - Max error rate is 3%
    basic_user_requests()
        .tps(5_000)
        .latency(Duration::from_millis(20), 0.95)
        .error_rate(0.03)
        .duration(Duration::from_secs(3600))
        .await;
}

// A Scenario is just an async Rust function, and
// can contain any complex logic you need.
#[scenario]
async fn basic_user_requests() {
    let client = reqwest::Client::new();
    loop {
        let _ = call_api(&client).await;
    }
}

// A Transaction is also just an async Rust function, and
// provides flexibility with what you want Balter to measure
// and constrain on.
#[transaction]
async fn call_api(client: &Client) -> Result<(), Error> {
    let res = client.post("https://example.com")
        .json(data)
        .send()
        .await;

    if res.status.is_success() {
        Ok(())
    } else {
        Err(Error::BAD_REQUEST)
    }
}
```

# Composability

# Distributed Runtime (Experimental)

