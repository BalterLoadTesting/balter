+++
title = "Balter v0.5.0"
date = "2024-04-12"
+++

# Balter v0.5.0

The release of Balter v0.5.0 comes with major core refactors and a major feature which rounds out the basic functionality for single-server operation.

# Latency Controller Added

You can now limit a Scenario based on latency measurements. Currently this takes both a latency value as well as a quantile (eg. p90):

```rust
foo_scenario()
    .latency(Duration::from_millis(20), 0.95) // a p95 latency of 20ms
    .await;
```

# Duration is No Longer Required

A `.duration()` call was previously required for a Scenario. Now it is not needed, and instead will default to running indefinitely.

```rust
// Runs indefinitely
foo_scenario()
    .tps(10_000)
    .await;
```

# Scenario Overhaul

The Scenario running logic was completely overhauled and simplified. This was done for a number of reasons, but the biggest benefit is a far more intuitive and powerful API. Scenarios can now takes as many constraints as you want, rather than just one:
```rust
// Limit with all of:
// - Max TPS of 10,000
// - Max p99 latency of 20ms
// - Max error rate of 3%
foo_scenario()
    .tps(10_000)
    .error_rate(0.03)
    .latency(Duration::from_millis(20), 0.99)
    .duration(Duration::from_secs(360))
    .await
```

# A Website

Balter now has a website! This will make it far easier to have long-form documentation and keep it updated more rapidly.
