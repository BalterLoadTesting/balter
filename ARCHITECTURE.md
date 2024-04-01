# Architecture

## Directory Overview

Code:

- `balter`
    - The primary logic for Balter. Includes all scenario running code, transaction hooks, controllers, etc. Default spot to look for code.
- `balter-runtime`
    - All logic related to the distributed functionality of Balter. Includes the Runtime, gossip protocol and anything else of this nature.
- `balter-core`
    - Shared primitives. Primarily just basic structs and definitions.
- `balter-macros`
    - Procedural macros for Balter, specifically `#[scenario]` and `#[transaction]`

Testing:

- `tests`
    - Integration tests for Balter. See README under developer notes for how to run.
- `benchmark`
    - WIP benchmarking suite for performance testing Balter.
- `mock-service`
    - A mock service to run Balter against. It exposes various APIs which trigger different balter behavior (error rates, latency, etc.)

Miscellaneous

- `examples`
    - WIP examples for various ways to use Balter.
- `shell.nix`
    - Nix support for all dependencies
- `Justfile`
    - Similar to Make, just a command runner
- `dashboards`
    - Grafana dashboard definitions for the integration tests to make debugging controller logic easier.
- `prometheus.yml`
    - Prometheus configuration for integration test suites
