use std::num::NonZeroU32;
use std::time::Duration;

/// Minimal Run Statistics for a given Scenario
///
/// Provides a sliver of the statistics available from a given Scenario run. More stats will be
/// added over time.
///
/// TODO:
/// - Error Rate
/// - Measured TPS (Quantiles)
pub struct RunStatistics {
    pub concurrency: usize,
    pub goal_tps: NonZeroU32,
    pub actual_tps: f64,
    pub latency_p50: Duration,
    pub latency_p90: Duration,
    pub latency_p99: Duration,
    pub error_rate: f64,
    pub tps_limited: bool,
}
