use balter::prelude::*;
use metrics_exporter_prometheus::PrometheusBuilder;
use reqwest::Client;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use tracing_subscriber::FmtSubscriber;
use std::sync::OnceLock;
use anyhow::Result;

static CLIENT: OnceLock<Client> = OnceLock::new();

#[tokio::main]
async fn main() {
    FmtSubscriber::builder()
        .with_env_filter("balter=debug")
        .init();

    PrometheusBuilder::new()
        .with_http_listener("0.0.0.0:8002".parse::<SocketAddr>().unwrap())
        .install()
        .unwrap();

    root_scenario().await;
}

#[scenario]
async fn root_scenario() {
    scenario_a()
        .tps(10_000)
        .duration(Duration::from_secs(45))
        .await;

    sleep(Duration::from_millis(10_000)).await;

    scenario_a_limited()
        .tps(10_000)
        .duration(Duration::from_secs(75))
        .await;

    sleep(Duration::from_millis(20_000)).await;

    scenario_b()
        .saturate()
        .duration(Duration::from_secs(120))
        .await;

    sleep(Duration::from_millis(5000)).await;
}

#[scenario]
async fn scenario_a() {
    let _ = transaction_a().await;
}

#[transaction]
async fn transaction_a() -> Result<()> {
    let client = CLIENT.get_or_init(Client::new);
    client.get("http://0.0.0.0:3002/delay/ms/10").send().await?;
    Ok(())
}

#[scenario]
async fn scenario_a_limited() {
    let _ = transaction_a_limited().await;
}

#[transaction]
async fn transaction_a_limited() -> Result<()> {
    let client = CLIENT.get_or_init(Client::new);
    client.get("http://0.0.0.0:3002/limited/7000").send().await?;
    Ok(())
}

#[scenario]
async fn scenario_b() {
    let _ = transaction_b().await;
}

#[transaction]
async fn transaction_b() -> Result<()> {
    let client = CLIENT.get_or_init(Client::new);
    let res = client.get("http://0.0.0.0:3002/max/3500").send().await?;

    if res.status().is_success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Some error"))
    }
}
