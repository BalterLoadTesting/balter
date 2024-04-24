use balter::prelude::*;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::hint::black_box;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=trace")
        .init();

    PrometheusBuilder::new()
        .with_http_listener("0.0.0.0:8002".parse::<SocketAddr>().unwrap())
        .install()
        .unwrap();

    let stats = scenario_a()
        .tps(70_000)
        .await;

    println!("{stats:?}");
}

#[scenario]
async fn scenario_a() {
    loop {
        let _ = transaction_a().await;
    }
}

#[transaction]
async fn transaction_a() -> Result<(), ()> {
    sleep(Duration::from_millis(10)).await;
    black_box(Ok(()))
}
