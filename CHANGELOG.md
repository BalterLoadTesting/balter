# CHANGELOG

## Balter 0.3.0  (balter-macros 0.2.0)

### Overhauled TPS sampling mechanism
After a ton of experimentation with different ways to measure TPS, I settled upon having
long-running tasks which all coordinate with the primary task via Atomics, and using ArcSwap
for anything that could not be done via an Atomic. This seems to hit the sweet spot of
accurate measurements and performance.

### Pin<Box<impl Future>> no longer in the hot path!

This is a major refactor which moves all Pin<Box<T>>'s out of the hot path. I am planning to
put together more documentation and benchmarks surrounding this change, since it surprised me
at the cost of Pin<Box<T>>. With the minimal benchmarking I've done, the performance
difference is fairly massive!

Unfortunately, the cost is that I've broken the distributed runtime functionality. It should
be a relatively simple fix to patch up the types, but I want to focus on stabalizing the
various controller logic first.

### Minor Changes

- Added a `direct()` scenario which is primarily useful for development.
- Added some benchmarking code and a section for thorough benchmarking in the codebase.
- Various minor bug fixes.
