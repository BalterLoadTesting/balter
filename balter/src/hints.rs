/// User provided hints for setting autoscaling parameters.
///
/// Balter attempts to find the optimal values for all parameters, however sometimes the control
/// loops can take a while to stabalize. These are user-provided hints (see [crate::Scenario#method.hint])
pub enum Hint {
    /// Provide the starting concurrency value. Useful for Scenarios with low TPS (which Balter can
    /// take a long time to stablize on).
    Concurrency(usize),
}
