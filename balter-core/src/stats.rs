use std::time::Duration;

/// Run Statistics for a given Scenario
#[derive(Debug, Default, Clone)]
pub struct RunStatistics {
    pub concurrency: usize,
    pub goal_tps: u32,
    pub actual_tps: f64,
    pub latency_p50: Duration,
    pub latency_p90: Duration,
    pub latency_p95: Duration,
    pub latency_p99: Duration,
    pub error_rate: f64,
    pub tps_limited: bool,
}
