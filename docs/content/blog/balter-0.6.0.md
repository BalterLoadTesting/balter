+++
title = "Balter v0.6.0"
date = "2024-05-10"
authors = ["Byron Wasti"]
+++

The release of Balter v0.6.0 rewrites the entire sampling logic and adds the concept of `hints` to running Scenarios.

# Sampling Logic Rewrite

The sampling logic used in Balter was written at the start of the project, and has evolved with duct-tape style engineering. Unfortunately this meant that it provided pretty crappy data to the controllers, and likely was leading to various hard-to-track down issues.

Now, the sampling logic provides far more statistically valid data which can be used to make more accurate decisions with. Rather than returning data at a fixed interval, the sampler will determine when the data has converged on certain values (specifically that the mean TPS has stabilized). It will also automatically adjust sampling intervals to ensure that low-TPS situations still have valid data collected.

# Hints

In an effort to make Balter more flexible, you can now provide "hints" to the Scenarios being run. For example:

```rust
my_scenario()
    .tps(10_000)
    .hint(Hint::Concurrency(100))
    .await;
```

This will have Balter start off with a concurrency of 100 for that Scenario, rather than the default of 10. Balter will continue to autoscale values, but this provides options for speeding up convergence.

Currently there is only a single hint available, `Hint::Concurrency`, but more will be added.
