use std::num::NonZeroU32;

/// Minimal Run Statistics for a given Scenario
///
/// Provides a sliver of the statistics available from a given Scenario run. More stats will be
/// added over time.
///
/// TODO:
/// - Error Rate
/// - Measured TPS (Quantiles)
#[non_exhaustive]
pub struct RunStatistics {
    pub concurrency: usize,
    pub goal_tps: NonZeroU32,
    pub stable: bool,
}
