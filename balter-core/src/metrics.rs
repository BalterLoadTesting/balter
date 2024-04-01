#[derive(Copy, Clone)]
pub struct TransactionLabels {
    pub success: &'static str,
    pub error: &'static str,
    pub latency: &'static str,
}

#[macro_export]
macro_rules! generate_labels {
    ($base_name:expr) => {
        ::balter::core::TransactionLabels {
            success: concat!(stringify!($base_name), "_success"),
            error: concat!(stringify!($base_name), "_error"),
            latency: concat!(stringify!($base_name), "_latency"),
        }
    };
}
