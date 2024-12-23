# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.1](https://github.com/BalterLoadTesting/balter/compare/balter-v0.8.0...balter-v0.8.1) - 2024-12-23

### Other

- Simplify by checking for 0.0 before infinity
- concurrency adjustment should not happen when mean is 0

## [0.8.0](https://github.com/BalterLoadTesting/balter/compare/balter-v0.7.0...balter-v0.8.0) - 2024-07-23

### Other
- Add more hints
- Fix issue with NaN measurements in Latency readings
- Add `#[allow(unused)]` for CI
