+++
title = "Index"
template = "index.html"
+++

# Balter
<b>B</b>alter, <b>A</b> <b>L</b>oad <b>T</b>est<b>ER</b>, is a distributed load testing framework designed to make testing high-traffic scenarios easy. Load test scenarios are written using standard Rust code, with a few special attributes thrown in.

Balter makes no assumptions about the service under test, and can be used for a variety of use-cases, from HTTP services to local key-value stores, and written in any language (Rust or not). At its core, Balter simply provides tooling to take an arbitrary Rust function and run it in parallel at scale with various constraints applied.

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

A Scenario 

### Current Restrictions
- `#[scenario]` can only be used on functions which take and return no arguments ([issue #1](https://github.com/BalterLoadTesting/balter/issues/1))

## Transactions

# Composability

# Distributed Runtime (Experimental)

# Sponsor

